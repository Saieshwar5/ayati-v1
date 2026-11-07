# Voice transcription (schema 95)

Agents read voice notes through their transcripts, and every agent now learns which
files a message carries. The controller sends each recording to a speech-to-text model
once, in the background after upload. Built 2026-11-07. Off unless configured.

## Turning it on

| Where | Setting |
| --- | --- |
| Controller | `AYATI_MODEL_PROVIDERS_FILE`, as for the [model gateway](model-gateway.md). The transcription provider needs its own entry because it can have its own address. |
| Controller | `AYATI_TRANSCRIPTION_MODEL=provider:model`. The provider must be in the providers file with a readable key file. Without the providers file, the setting is refused, and a provider that is missing or unusable stops startup. |

Fireworks serves Whisper on a separate host with the same API key:

```json
[{"provider": "fireworks-audio",
  "base_url": "https://audio-prod.us-virginia-1.direct.fireworks.ai/v1",
  "key_file": "/private/fireworks-key"}]
```

```sh
AYATI_TRANSCRIPTION_MODEL=fireworks-audio:whisper-v3-turbo   # or whisper-v3
```

Any provider with OpenAI's `POST {base_url}/audio/transcriptions` works the same way
(for example OpenAI with `whisper-1`). The controller sends a multipart form with
`file` (the audio, its name and type), `model` and `response_format=json`, and reads
`text` from the answer (and `duration` when the provider gives it). Keys stay in the
key file, read on each request; the controller still refuses model keys in its environment.

**Privacy:** each recording's audio and file name go to the configured provider, like
any model request. The audio, transcripts and keys are never logged.

## How it works

1. An audio upload (`audio/webm`, `audio/ogg`, `audio/mp4` or `audio/wav`) queues a row
   in `relay_object_transcripts` in the transaction that saves the file, and wakes this
   controller's worker. Other files queue nothing.
2. Every controller runs the worker. An upload is announced to all of them
   (schema 97), and a worker also wakes when a retry pause or lease ends, or
   after 60 seconds (it looked every 5 seconds before schema 97). It takes up to
   4 transcripts at once with `FOR UPDATE SKIP LOCKED` and a 150-second lease in
   the database, so two replicas never send the same recording.
3. The worker reads the bytes from file storage (checked against the upload's size and
   checksum) and reads the length from the file where it says (WAV, Ogg, WebM, MP4).
   A recording over 5 minutes (10 MiB at most) is never sent. Each request has 90 seconds.
4. A busy, unreachable or slow provider (408, 429, 5xx, timeouts) is tried again after
   10 and 20 seconds; after 3 tries the transcript has failed. A lease that runs out
   counts as a try. Other refusals fail at once. Results save only under the lease
   that took them.
5. When a transcript is saved or fails, open conversations reload the message through
   the usual change stream.

| Status | Meaning |
| --- | --- |
| `pending` | Waiting or being transcribed |
| `ready` | `transcript` holds the text (up to 16,000 characters) |
| `failed` | Too long, refused, or still failing after 3 tries |
| `not_enabled` | Uploaded while transcription was off (no row) |

## What agents see

The conversation evidence lists each message's attachments as `{name, mime, size}`.
Audio adds `transcript_status`, `duration_seconds` when known, and `transcript` (up to
8,000 characters, with `transcript_truncated`). A message that only shares files
reaches the agent with empty content instead of the "Shared files" placeholder the web
client stores. Agents are told that names and transcripts are untrusted data like
message content, that transcripts may mishear words, and to say they can't listen to a
voice note when it has no transcript. Conversations without attachments get no extra text.

**Waiting:** a request whose message has a pending transcript is not offered to an
executor until the transcript is saved, or for at most 30 seconds after it was asked
(`pool_candidate.sql`). It then runs with whatever status the transcript has. The job
stays queued while it waits, so no executor slot is held.

## What people see

Message history and live updates add `transcript_status`, `transcript` and
`duration_ms` to audio attachments, so only people who can read the message see its
transcript. Under the voice message player ([Unified Chat](unified-chat.md#voice-messages-d245)),
a **≡ Transcript ⌄** line opens the text; "Transcribing…" and "Transcript unavailable" show
the other states. Nothing extra shows when transcription is off. The player shows
`duration_ms` as the length when known, else the decoded recording's length. The composer
stops recordings at 5 minutes, so they stay within the worker's limit.

## Usage and cost

Each successful call is a `relay_model_usage` row (`assignment` "transcription", the
uploader as person, latency, no tokens), so [model operations](model-operations.md)
show its count and speed, and provider failures appear with the others. Providers price
audio by the minute, which tokens don't capture: charges and costs are 0, and the length
is kept in `relay_object_transcripts.duration_ms`. Workspace budgets don't apply yet.

## Checks

- Unit: the multipart form and answer parsing against a loopback provider (key,
  fields, refusals, retries, time limit); recording lengths; configuration rules.
- PostgreSQL (`transcripts_tests`, `transcription::worker_tests`): an audio upload queues
  one transcript and two racing controllers take each once; text is saved and announced;
  retries end in "failed"; an agent request waits, then reads the transcript, or runs
  with "pending" after 30 seconds; without the setting, attachments still reach agents as
  `not_enabled`; only people who can read the message see the transcript; the 94 → 95
  upgrade; a worker round sends a short note and refuses one over 5 minutes.
- Web (`web/tests/voice-transcript.test.mjs`): the transcript line's states; the recorder and
  player are covered in `voice-recorder.test.mjs` and `voice-player.test.mjs`.

## Not yet

- A live call to Fireworks: their documentation page was unreachable, so the address
  and model stay configurable. It is checked in the next cloud window.
- Searching older history (`search_conversation_history`) doesn't show attachments.
- Keys sealed in Cuckoo Control, the provider switch and budgets don't apply to
  transcription, as for the Library's models.
