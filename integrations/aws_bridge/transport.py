"""Maintained SDK signing and bounded HTTP forwarding; no redirects or response-body logs."""
from urllib.parse import urlsplit

import boto3
from botocore.auth import SigV4Auth
from botocore.awsrequest import AWSRequest
from botocore.config import Config
import urllib3

from .config import cloud_identity, gateway_url


class Transport:
    def __init__(self, gateway=None, session=None):
        if not hasattr(urllib3.response.HTTPResponse, "read1"):
            raise RuntimeError("Install the locked AWS bridge dependencies: urllib3 read1 is required")
        self.session = session or boto3.Session(region_name="ap-south-1")
        self.gateway = gateway_url(gateway) if gateway else None
        self.http = urllib3.PoolManager(num_pools=8, maxsize=24, retries=False,
                                       timeout=urllib3.Timeout(connect=3, read=10))

    def request(self, method, url, body=b"", headers=None, signed=False):
        values = {"Content-Type": "application/json", **(headers or {})}
        if signed:
            if not self.gateway or not url.startswith(self.gateway + "/v1/"):
                raise ValueError("Signing is limited to the configured gateway")
            credentials = self.session.get_credentials()
            if credentials is None:
                raise PermissionError("AWS task credentials unavailable")
            request = AWSRequest(method=method, url=url, data=body, headers=values)
            SigV4Auth(credentials.get_frozen_credentials(), "execute-api", "ap-south-1").add_auth(request)
            values = dict(request.headers.items())
        # urllib3 verifies HTTPS certificates. The only plaintext destinations
        # used by callers are fixed loopback services on the same machine.
        parts = urlsplit(url)
        if parts.scheme != "https" and not (parts.scheme == "http" and parts.hostname == "127.0.0.1"):
            raise ValueError("TLS or fixed loopback destination required")
        return self.http.request(method, url, body=body if method != "GET" else None,
                                 headers=values, preload_content=False, redirect=False, retries=False)


class TaskVerifier:
    def __init__(self, role, cluster, release, session=None):
        self.role, self.cluster, self.release = role, cluster, release
        session = session or boto3.Session(region_name="ap-south-1")
        self.ecs = session.client("ecs", config=Config(connect_timeout=3, read_timeout=5,
                                                      retries={"total_max_attempts": 1}))

    def verify(self, principal):
        arn, task_id = cloud_identity(principal, self.role, self.cluster)
        response = self.ecs.describe_tasks(cluster=self.cluster, tasks=[arn])
        tasks = response.get("tasks", [])
        if response.get("failures") or len(tasks) != 1:
            raise PermissionError("Task identity unavailable")
        task = tasks[0]
        if task.get("taskArn") != arn or task.get("clusterArn") != self.cluster or task.get("lastStatus") != "RUNNING" or task.get("taskDefinitionArn") != self.release:
            raise PermissionError("Wrong task, state or release")
        definition = self.ecs.describe_task_definition(taskDefinition=self.release)["taskDefinition"]
        if definition.get("taskRoleArn") != self.role:
            raise PermissionError("Wrong packaged task role")
        return arn
