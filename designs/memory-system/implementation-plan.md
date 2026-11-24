# Memory system: implementation plan (proposal, 2026-11-24)

Build plan for [the memory system](README.md). Nothing is built. Names of tables,
tools and modules are proposals; numbers are starting values. Each step ends with
something a person can see and a PostgreSQL test set, as other features have.

## What exists and what happens to it

| Exists today | Where | What happens |
| --- | --- | --- |
| History tools `search_conversation_history`, `read_conversation_history` | `src/identity/postgres/conversation_history.rs` (literal match over at most 256 messages) | **Changed** in step 1: ranked word search over the whole conversation using the message index of schema 92, filters for author and dates. The scope stays bound to the running task. |
| Message full-text index | `relay_chat_messages_search` (schema 92) | **Reused** by step 1. |
| Assistant memory | `relay_assistant_memories`, `assistant_memory.rs`, tools `remember_about_person`, `search_memory`, `forget_memory` | **Kept as it is.** In step 4 an assistant also reads workspace memory. Moving it onto the engine is step 6, optional. |
| Agent notes and lessons | `relay_agent_knowledge`, `relay_knowledge_proposals`, `agent_learning` | **Kept.** They are rules a person wrote or approved. Step 3 adds memory suggestions to the same review list. The names-and-numbers filter is reused for notes that go above "here". |
| Room brief | `relay_working_context`, `save_working_context` | **Kept.** It is the short-term picture of a room. Fencing on audience change is the pattern the room shelf copies. |
| Context assembly and preview | `conversation_context.sql`, `context_preview.rs`, `system_prompt.rs` | **Changed** in step 2: a "Memory" block with fixed sizes, a "Memory here" section in the preview, and the where-to-look ladder in the instructions. |
| Activity request details | `skill_runtime.rs` ("Used skill: …" from `relay_skill_loads`) | **Pattern reused:** memory lines come from a new `relay_memory_uses`. |
| Library meaning search | `library/meaning.rs`, `relay_library_embeddings`, `AYATI_LIBRARY_EMBEDDING_MODEL` | **Reused** in step 5: the embedding call becomes a shared helper; memory gets its own embeddings table. |
| Learning model setting | `AYATI_LEARNING_MODEL`, the controller task in `agent_learning.rs` | **Reused** for the tidy-up task in step 5. |
| Owner view | `search_my_work`, `read_my_work` | **Kept.** It reads Activity; memory lines appear there like other steps. |
| Member removal, channel audience changes | foreign keys, brief fencing | **Extended:** person shelves delete with the member; room shelves fence when a room becomes more private. |

## New systems

**1. The memory store.** One set of tables for every level.

- `relay_memories`: workspace, id, `scope` (`workspace`, `agent`, `room`,
  `person`), agent, chat, user_id (each set only where the scope needs it),
  topic, kind, text (1–300 characters), source (`agent`, `person`, `admin`,
  `tidy`), origin (the request that kept it), learned_in (the conversation it
  came from), strength 0–100, used count, last used, until, state (`active`,
  `fading`, `archived`, `held`, `ended`, `folded`), pinned, replaced_by, created,
  ended. A check constraint ties the key columns to the scope. Word index as on
  `relay_assistant_memories`.
- `relay_memory_topics`: one summary per topic per shelf, with its revision.
- `relay_memory_uses`: one row per action on a request (`given`, `searched`,
  `found`, `kept`, `ended`), with the query and the count. Feeds Activity,
  strength and "Looked for, not found".
- `relay_memory_proposals`: notes waiting for a person (wider filing, outside
  sources, disagreements).
- `relay_memory_changes`: what the tidy-up did, for the Changes list and undo.

**2. The scope rule.** One function, given the running task, returns the shelves
that request may read and the one shelf it may write directly. Every read, search
and write goes through it. No tool argument can name a person, room or agent.
Replies that leave the workspace get only their own thread's shelf.

**3. Agent tools** (served by `ayati_control`, schemas in the guest crate):

- `remember {text, kind, topic?, until?, for?}` with `for` one of `here`
  (default), `my_job`, `workspace`. Same words on the same shelf are kept once.
- `end_memory {memory, reason?}` ends a note on a shelf the request may write.
- `search_memory {query, where?}` returns up to 15 labelled lines from every
  shelf the request may read. For an assistant the existing tool of this name
  also returns workspace lines.
- `search_conversation_history {query, from?, after?, before?}` (changed).

**4. The given block.** Built by the server for each attempt, labelled as
information and never instructions:

| Part | Size |
| --- | --- |
| Workspace essentials | 2,000 characters |
| Job essentials | 1,500 |
| This room's or person's top notes | 1,500 |
| Up to 3 matching topic summaries | 3,000 |

When more exists the block says how many, as assistant memory does.

**5. The tidy-up task.** A controller task like agent learning: expire, merge,
settle disagreements, summarise topics, fade, archive, delete, choose essentials.
It works inside one shelf at a time and never mixes shelves. Rules are in
[`../workspace-memory/README.md`](../workspace-memory/README.md).

**6. Screens.** Settings › Memory; Memory and About you on the agent page; the
"Remembered · Undo" line in chat; memory lines in Activity; "Memory here" in the
context preview. React, under `web/src/react/settings/memory/` and
`web/src/react/agents/profile/memory/`.

## Steps

Each step is useful alone and is released before the next starts.

### Step 1: better history search

- Ranked word search over the whole conversation, any of the words, prefix
  match, with `from`, `after`, `before`. Same source exclusions as today.
- `relay_memory_uses` is created here and records history searches too.
- Activity shows "Searched this conversation: '…' · n found".
- A person sees: the agent finds what they said three months ago.
- Tests: another conversation's message never found; excluded source answers
  never found; an old cursor cannot pass the task's reply sequence; since-joining
  still applies; a forged scope argument is refused.

### Step 2: room and person shelves

- The store, the scope rule, `remember` (only `here`), `end_memory`,
  `search_memory`, the given block for this conversation's shelf.
- Chat: "Remembered: … · Undo". Agent page: About you. Context preview: Memory
  here. Activity lines.
- Deleting with the member; fencing when a room becomes more private.
- A person sees: the agent remembers how they like answers, and they can undo it.
- Tests: Meera's note never given or found in Ravi's chat or a channel; a room
  note never given in another room or a direct chat; an outside-customer thread
  reads only its own shelf; Undo by the person; removal of the member deletes;
  a narrowed room fences; a schedule's run cannot keep a note directly; the same
  words kept once; a note is labelled information.

### Step 3: the job shelf

- `for: my_job`, direct only from a public channel or the agent's own tool
  result, otherwise a suggestion in Lessons from its work.
- Agent page: Memory, for owners and admins, with pin, correct, end, remove.
- An owner sees: what the agent has learned about its job, with sources.
- Tests: a note from a private chat is only a suggestion and carries no name,
  address, long number or amount; approval files it; non-owners are refused.

### Step 4: workspace memory

- `for: workspace`, direct only from workspace-visible sources. The Needs your
  answer queue. Settings › Memory with Topics as plain groups, Notes, Essentials.
- Every shared agent and every assistant is given the essentials and can search.
- An admin sees: one page with what all agents know about the business.
- Tests: members get `404` on every memory route; a note from a restricted
  Library item is refused; an outside-source note is unused until kept; an
  assistant reads and cannot write; customer-facing replies get none.

### Step 5: the memory looks after itself

- Strength changes, fading by kind, archive and delete, merging, disagreements,
  topic summaries, essentials chosen each night, the Changes list with undo.
- Meaning search for memory (pgvector, optional as in the Library).
- An admin sees: Fading, Archive and Changes; summaries on topics.
- Tests: nothing merged or summarised across shelves; a pinned note never fades;
  an ended note is kept beside its replacement; undo restores a merge; a held
  note is not given; the task skips when the model budget is used up.

### Step 6: measuring and the later parts

- "Looked for, not found", and the share of uses followed by a correction.
- In a direct chat, searching channels both the person and the agent are in.
- Meaning search over messages, only for conversations with an agent.
- Optional: move assistant memory onto the engine so it gains fading.

## Order and reasons

History search first: the tools exist, the scope is already safe, and it adds the
record that every later step needs. Room and person shelves second: the rooms
already enforce their boundaries. Workspace memory fourth, not first: it has the
widest audience, so it is built on an engine already proven on small audiences.
The tidy-up fifth: its numbers cannot be set without real notes.

## Limits

| Thing | Starting value |
| --- | --- |
| A note | 300 characters |
| Person shelf | 500 active notes |
| Room shelf | 1,000 |
| Job shelf | 2,000 |
| Workspace | 50,000 |
| Search result | 15 lines |
| Given with a request | 8,000 characters |

## To confirm before step 2

- Owners and admins open a person shelf under Activity's rule (audited), as the
  README proposes, or not at all.
- Room members read the room shelf in Conversation context, or only owners and
  admins do.
- Whether "Remembered · Undo" is shown for room notes to everyone in the room.

## Not planned

A graph database or an outside memory service: the per-room access rules would
have to be built a second time, the reason assistant memory was built in
PostgreSQL. Learning from private chats into wider memory without a person.
Memory as instructions.
