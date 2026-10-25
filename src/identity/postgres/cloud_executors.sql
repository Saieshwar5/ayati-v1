-- The trusted IAM front door binds one packaged executor incarnation to one ECS task.
ALTER TABLE relay_executors ADD COLUMN cloud_bound BOOLEAN NOT NULL DEFAULT FALSE;
CREATE TABLE relay_cloud_executors (
 executor TEXT PRIMARY KEY REFERENCES relay_executors(id),
 incarnation TEXT NOT NULL,
 task_arn TEXT NOT NULL UNIQUE,
 principal_arn TEXT NOT NULL,
 release_arn TEXT NOT NULL,
 FOREIGN KEY(executor,incarnation) REFERENCES relay_executors(id,incarnation),
 CHECK(length(task_arn) <= 256 AND length(principal_arn) <= 256 AND length(release_arn) <= 256)
);
