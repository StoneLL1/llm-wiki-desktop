---
title: MCP (Model Context Protocol)
created: 2026-04-23
updated: 2026-05-17
type: entity
tags: [architecture, tool, open-source]
sources:
  - claude-code-10-more-worthwhile-skills
  - claude-code-mobile-remote
  - figma-vs-pencil-claude-code
  - harness-engineering-source-code
  - build-ai-agent-framework
  - multi-agent-collaboration-guide
  - raw/articles/2026-05-14-anthropic-financial-skills.md
---

# MCP (Model Context Protocol)

## Overview

The Model Context Protocol (MCP) is [[anthropic]]'s open protocol for connecting AI models to external tools and data sources. MCP provides a standardized way for AI agents like [[claude-code]] to interact with browsers, design tools, databases, APIs, and other external services. It appears in 9 articles across the corpus, making it one of the most referenced frameworks in the wiki.

## Architecture

### Client-Server Model
MCP follows a client-server architecture:

- **MCP Client**: Integrated into the AI agent (e.g., Claude Code contains an MCP client)
- **MCP Server**: A separate process that exposes tools and resources to the client
- **Protocol**: Standardized communication protocol between client and server

This separation allows AI agents to gain new capabilities by connecting to MCP servers without modifying their core code.

### Tool Integration
MCP servers expose "tools" that AI agents can invoke:

- **Browser automation**: Playwright MCP for web browsing and testing
- **Design tools**: Figma MCP, Pencil MCP for design file access
- **File system**: Local and remote file operations
- **API access**: REST and GraphQL API integration
- **Database queries**: SQL and NoSQL database access

### Resource Access
Beyond tools, MCP servers can expose "resources" — data sources that the AI model can read:

- File contents and metadata
- Database records
- API responses
- Real-time data streams

## Key MCP Servers

### Playwright MCP
Browser automation server that enables AI agents to:

- Navigate web pages
- Fill forms and click elements
- Capture screenshots and page content
- Test web applications
- Scrape web data

Referenced in playwright-mcp-browser-automation.

### Figma MCP
Connects AI agents to Figma design files:

- Read design specifications
- Access component libraries
- Export design assets
- Bridge design-to-code workflows

Referenced in figma-vs-pencil-claude-code.

### Pencil MCP
Alternative design tool MCP server:

- Connects to the Pencil design tool
- Provides design file access for Claude Code
- Compared with Figma MCP for design workflows

Referenced in figma-vs-pencil-claude-code.

## Security Considerations

MCP server deployments require careful security attention (mcp-server-security):

- **Access control**: Who can invoke which tools
- **Data isolation**: Preventing unauthorized data access between sessions
- **Input validation**: Sanitizing inputs to prevent injection attacks
- **Rate limiting**: Preventing abuse of tool invocations
- **Audit logging**: Tracking tool usage for accountability

Security is particularly important because MCP servers often have access to sensitive resources like filesystems, databases, and external APIs.

## Use Cases

### Claude Code Tool Integration
[[claude-code]] uses MCP as its primary mechanism for external tool integration. Through MCP, Claude Code can:

- Access design tools (Figma, Pencil) for design-to-code workflows
- Automate browsers via Playwright for testing and web scraping
- Interact with version control systems
- Access databases and APIs
- Extend capabilities without modifying Claude Code itself

### Multi-Agent Communication
MCP can facilitate communication between multiple AI agents (multi-agent-collaboration-guide):

- Shared tool access across agents
- Standardized resource sharing
- Inter-agent communication protocols
- Coordination of multi-agent workflows

### AI Agent Frameworks
MCP is referenced as a key component in building AI agent frameworks (build-ai-agent-framework):

- Provides standardized tool integration
- Enables modular agent architectures
- Supports the [[multi-agent-collaboration]] paradigm
- Compatible with multiple AI model providers

### Engineering Source Code Access
MCP can be used to give AI agents controlled access to engineering source code (harness-engineering-source-code):

- Code repository exploration
- Automated code review
- Documentation generation
- Refactoring assistance

## Design Philosophy

MCP embodies several key design principles:

1. **Standardization**: One protocol for all tool integrations, reducing fragmentation
2. **Modularity**: Tools are independent servers that can be developed and deployed separately
3. **Security**: Built-in security model for access control and isolation
4. **Extensibility**: New tools can be added without modifying the AI agent
5. **Interoperability**: Works across different AI models and agent platforms

## Relationship to Agent Skills

MCP and the [[skills]] (SKILL.md) ecosystem serve complementary purposes:

- **MCP**: Connects agents to external tools and data sources (infrastructure layer)
- **Skills**: Define agent capabilities and workflows (application layer)
- **Together**: Skills can reference MCP tools, and MCP servers can be loaded as skill dependencies

## Comparison with Alternatives

| Protocol | Creator | Scope | Status |
|----------|---------|-------|--------|
| MCP | Anthropic | Tool + resource integration | Active, growing |
| OpenAI Function Calling | OpenAI | Function invocation | Proprietary |
| LangChain Tools | Community | Tool wrappers | Framework-specific |
| Direct API | Various | Custom integrations | No standardization |

## Key Relationships

- Created by [[anthropic]]
- Primary integration mechanism for [[claude-code]]
- Used with [[figma]] and Pencil for design tool access
- Referenced in [[mcp]] for agent development
- Supports [[multi-agent-collaboration]] patterns
- Related to [[computer-use-agent]] for desktop automation
- Complementary to [[skills]] ecosystem

## Sources

- claude-code-10-more-worthwhile-skills — Claude Code MCP tool integration
- claude-code-mobile-remote — Remote MCP server access
- figma-vs-pencil-claude-code — Design tool MCP comparison
- harness-engineering-source-code — Engineering source code access via MCP
- build-ai-agent-framework — MCP in agent framework architecture
- multi-agent-collaboration-guide — MCP for multi-agent communication

## Ecosystem Updates

For the latest MCP ecosystem updates (Mirage, financial skills, new integrations), see [[mcp-ecosystem]].
