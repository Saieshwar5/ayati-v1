# Web search for agents

Proposed 2026-11-08; the owner asked to start building it the same day. First slice
([runtime/web-search.md](../../runtime/web-search.md)): the provider contract with
Tavily, Exa, Perplexity and Parallel, the agent tools, cited pages, limits, the create
step (screen 1) and the switch on the Tools tab (screen 2 without use, cost and recent
searches). Not built yet: use and recent searches on the Tools tab, Activity details
(screen 3, right) and the Cuckoo Control page (screen 4). Editable source: [mockups.html](mockups.html), opened in a browser
from this folder. The example company and web pages are fictional; provider prices
are examples.

## The idea (owner, 2026-11-08)

- **Every agent has web search, on by default.** Its owner or managers switch it
  off or on; it is the agent's choice.
- **People never see who provides it.** They just use it. No provider name, key or
  price list appears anywhere in a workspace.
- **Several providers from the start:** Tavily, Exa, Perplexity Search API and
  Parallel. Adding another later must be easy.
- **No browsing.** The agent asks a search service and gets ranked results with
  clean page text. It never clicks through sites. A real browser (for logins and
  forms) stays "Coming later".

| File | Screen and notes |
| --- | --- |
| `1-create-tools.png` | Create step 2, renamed from Connectors to **Tools**. (1) What the agent can use. (2) A new **Built into Cuckoo** group above the admin's apps. (3) Web search, on by default. (4) What it is good for and when to switch it off. (5) What leaves Cuckoo. Controls (step 5) has nothing to set: it only reads. |
| `2-agent-tools-tab.png` | Agent page, Tools tab. (1) The same group. (2) One switch for the owner and managers. (3) This month's searches, pages and cost inside the existing budget. (4) Recent searches; words from conversations the viewer can't see stay hidden. (5) Who can change it; the change applies from the next message. |
| `3-chat-answer.png` | Chat. (1) A progress line: what was searched, pages read. (2) "Web pages used for this answer", built by Cuckoo from its records, listed apart from Library documents. (3)–(4) Activity request details: searches, pages, cost. (5) No provider shown. |
| `4-control-providers.png` | Cuckoo Control (operators only). (1) A new Web search group beside Models. (2) A card per provider: search, read pages or both; key, price, health. (3) Routing: one order for searching, one for reading pages, with fallback. (4) Emergency off. (5) Limits for every request. |

## How providers stay easy to add

Copying what already works for connectors (`ConnectorProvider`) and models (the
gateway): Cuckoo owns the tools, records, limits and money; a provider only answers.

```
Agent ── web_search / read_web_page ──▶ controller ──▶ router ──▶ Tavily | Exa | Perplexity | Parallel | …
         (Cuckoo's own tool names)      (switch, limits,  (order and fallback,
                                         budget, receipt)  set in Cuckoo Control)
```

- **One contract.** `src/identity/web_search/` would hold a `WebSearchProvider`
  trait with Cuckoo's own types: `search(query) -> results` and
  `read(url) -> page`, plus what the provider **can** do (search, read pages,
  filter by date or site). Perplexity Search API only searches, so it is never
  offered for reading pages.
- **One file per provider** (`tavily.rs`, `exa.rs`, `perplexity.rs`,
  `parallel.rs`) translates Cuckoo's request to the provider's API and back. Adding
  a provider is that file, one line registering it, its price, and passing a shared
  conformance suite against a local fake of its API, as connector providers do. No
  change to tools, screens, records or budgets.
- **Keys and prices** come from a providers file or are kept sealed in Cuckoo
  Control, like model keys. Executors never hold them, and never get internet.
- **Fallback is safe** because search only reads: a failed or slow call simply goes
  to the next provider. The record keeps which provider answered, for operators.
- **Pages are fetched by the provider, never by the controller**, which can reach
  the database and cloud internals.

## Rules that keep it safe

- **No leaking out.** `read_web_page` opens only links from this request's search
  results or ones a person wrote in the conversation, so a page can't trick an agent
  into sending private data to a site through a link.
- **Web pages are evidence, not instructions.** Existing Allow, Ask first and Block
  still stop every write an injected page might ask for.
- **Limits per request:** 10 searches, 10 pages, about 6,000 characters per read.
  Page text costs more in model tokens than the search itself.
- **Money:** web search counts in the existing monthly budgets. When a budget is
  used up, web search stops with the rest of the agent's work.
- **Privacy in Activity:** searches follow the visibility of the conversation they
  came from.

## Open questions

1. Can a workspace admin turn web search off for every agent (default on)?
2. Do existing agents get it on, or only new ones?
3. May replies sent outside the workspace (communication gateway) use web results?
4. Which provider is first for searching, and which for reading pages? A comparison
   on about 30 real questions should decide it.
5. Each provider's terms must allow use inside a paid product and showing page text;
   Cuckoo's privacy terms must list them as processors, even though the app doesn't.
