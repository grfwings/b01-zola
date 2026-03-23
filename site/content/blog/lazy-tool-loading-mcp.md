+++
title = "Lazy Tool Loading for MCP"
date = 2026-03-20

[taxonomies]
categories = ["Technical"]
tags = ["programming", "mcp"]
+++

MCP is a great protocol with a glaring issue: token usage. Large MCP servers like [Playwright MCP](https://github.com/microsoft/playwright-mcp) or [Chrome DevTools](https://github.com/ChromeDevTools/chrome-devtools-mcp) dump dozens of tools in context at runtime, burning valuable tokens. Ten MCP servers can burn 40,000+ tokens before the user has said a word.

Because of this, developers frequently pass on MCP servers in favor of command-line tools and Skills, because Skills support lazy loading meaning they consume far fewer tokens passively. Skills are great for workflows and instruction, but using a Skill+CLI tool for accessing structured data means you give up  authentication, real-time bidirectional communication, saved state, typed schemas, and all the rest MCP has to offer.

I initially wrote this blog post on an 11 hour flight from San Francisco to Seoul. After landing, I learned (unsurprisingly) that there are some ongoing efforts to address this:

*[Cloudflare](https://blog.cloudflare.com/code-mode-mcp/) implemented a lazy loading technique with Code Mode, which collapses 2,500+ endpoint APIs into a search() and execute() tool. This works, but the MCP layer loses visibility into individual operations, making client-side guardrails harder to implement.*

*[Amp](https://ampcode.com/news/lazy-load-mcp-with-skills) combined MCP with Agent Skills to create a technique for deferring MCP loading until a skill is invoked. This is brilliant, and the most similar to the original proposal of this blog. The only downsides are that it requires manual configuration per server and only works within Amp's skill system.*

*[Claude Code](https://x.com/trq212/status/2011523109871108570) shipped "MCP Tool Search" in January 2026, a client-side implementation that defers tool loading when schemas exceed 10K tokens. Anthropic then generalized this into the [Tool Search Tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool), an API-level feature that supports up to 10,000 deferred tools with regex and BM25 search. It works well, but it lives inside Anthropic's Messages API. Other LLM providers, local tool-calling setups, and non-Anthropic MCP clients don't benefit.*

*[SEP-1821](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1821) is an open proposal to add a query parameter to tools/list for server-side tool filtering. It addresses the query/subset part of this idea but still involves fetching full schemas in context.*

So, a lot of talented people are doing great work to mitigate the MCP token use issue. That said, all of these solutions live on the harness or platform level. If we push the logic one step up to the protocol, we get a solution that works for every MCP client and server regardless of platform. Here's what that could look like:

MCP servers already declare a `description` in `serverInfo` during initialization. We can use this for discovery, just like the description field in Agent Skills. The server author adds a summary of the server's capabilities, i.e. "mailmcp has tools for managing the user's email. It can read, write, and receive emails. Use mailmcp when..." Then we build on this with some small protocol changes:

1. A new `tools/search` method.
Servers declare search support in their capabilities: `tools: { search: true }`. When the model decides it needs a server's tools, the client calls `tools/search` with a natural language query:
```json
{
  "method": "tools/search",
  "params": {
    "query": "delete junk mail"
  }
}
```
The server returns lightweight tool stubs containing the tool's name, description, and annotation, but no `inputSchema` or `outputSchema`.
```json
{
  "result": {
    "tools": [
      {
        "name": "delete_emails",
        "description": "Delete emails matching a filter criteria"
      },
      {
        "name": "list_inbox",
        "description": "List emails in the user's inbox"
      }
    ]
  }
}
```

The MCP SDKs can provide a default `tools/search` implementation based on text matching over tool names and descriptions, or override with custom filtering logic if needed.

2. A `names` filter on `tools/list`.
Once the model has chosen a tool from the search results, the client fetches the full schema by calling `tools/list` with an explicit name filter:
```json
{
  "method": "tools/list",
  "params": {
    "names": ["delete_emails", "list_inbox"]
  }
}
```

This returns the complete tool definitions, including `inputSchema`, only for the requested tools.
```json
{
  "result": {
    "tools": [
      {
        "name": "delete_emails",
        "description": "Delete emails matching a filter criteria",
        "inputSchema": {
          "type": "object",
          "properties": {
            "filter": {
              "type": "string",
              "description": "Search filter for emails to delete"
            }
          },
          "required": ["filter"]
        }
      }
    ]
  }
}
```
The model now has everything it needs to construct valid arguments.

All of these changes are additive; Clients that don't support `tools/search` keep calling `tools/list` and get the full set. The `names` parameter on `tools/list` is optional, omitting it returns everything (which is the current behavior.)

Skills are great for behavior, workflows, and best practices, but MCP provides a much more attractive platform for large, complex systems. When you need OAuth scoping per user, structured error handling, or a server that pushes notifications when new data arrives, you need MCP. The token cost problem shouldn't lock people into specific platforms or push them away from MCP entirely. If you're interested in discussing this, please [contact me](https://www.betweenzeroand.one/about/) via twitter or [email](mailto:grfwings@protonmail.com) .

---

Kudos to [Mario Zechner](https://x.com/badlogicgames), [Rhys Sullivan](https://x.com/RhysSullivan), and [Dillon Mulroy](https://x.com/dillon_mulroy), as well as the Amp, Cloudflare, and Claude Code teams for serving as inspiration for this post.
