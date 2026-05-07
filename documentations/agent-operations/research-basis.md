# Research Basis

This document records the external sources and internal project sources used to produce this AI operating pack.

## External sources

- **OpenAI Prompting fundamentals** — https://openai.com/academy/prompting/
    - Usage: Prompt clarity, task outline, intended audience, and iterative refinement.
- **OpenAI API prompt engineering best practices
  ** — https://help.openai.com/en/articles/6654000-best-practices-for-promptengineering-with-the-openai-api
    - Usage: Instruction placement, delimiters, explicit output format, precise constraints, positive instructions.
- **OpenAI Agents SDK** — https://platform.openai.com/docs/guides/agents-sdk/
    - Usage: Agents with tools, handoffs, streaming, and tracing.
- **OpenAI Agents SDK Guardrails** — https://openai.github.io/openai-agents-js/guides/guardrails/
    - Usage: Input, output, and tool guardrails; tripwire behavior.
- **OpenAI Agents SDK Tools** — https://openai.github.io/openai-agents-js/guides/tools/
    - Usage: Hosted tools, function tools, agents as tools, MCP servers, and tool selection.
- **OpenAI Agents SDK Lifecycle Hooks** — https://openai.github.io/openai-agents-python/ref/lifecycle/
    - Usage: RunHooks and AgentHooks for LLM, tool, and handoff events.
- **OpenAI Agents SDK Tracing** — https://openai.github.io/openai-agents-js/guides/tracing/
    - Usage: Tracing of LLM generations, tools, guardrails, handoffs, and custom events.
- **OpenAI Prompt Injection Guidance** — https://openai.com/index/designing-agents-to-resist-prompt-injection/
    - Usage: Prompt injection as social-engineering-like manipulation; defense-in-depth.
- **Claude Prompt Engineering Overview** — https://docs.anthropic.com/en/docs/prompt-engineering
    - Usage: Success criteria, empirical evaluations, examples, roles, XML tags, chained prompts.
- **Claude XML Prompt Tags** — https://docs.anthropic.com/en/docs/build-with-claude/prompt-engineering/use-xml-tags
    - Usage: Structured prompt sections and parseable output.
- **Claude Tool Use** — https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/implement-tool-use
    - Usage: Detailed tool descriptions and JSON-schema parameter definitions.
- **Claude Code Subagents** — https://docs.claude.com/en/docs/claude-code/subagents
    - Usage: Task-specific subagents with separate context windows and restricted tools.
- **Claude Code Hooks** — https://code.claude.com/docs/en/hooks
    - Usage: Command, HTTP, MCP, prompt, and agent hooks across lifecycle events.
- **Claude Agent Skills** — https://docs.claude.com/en/docs/claude-code/skills
    - Usage: Skills as discoverable folders containing SKILL.md plus optional support files.
- **Claude Skill Authoring Best Practices
  ** — https://docs.claude.com/en/docs/agents-and-tools/agent-skills/best-practices
    - Usage: Focused skills, concise descriptions, progressive disclosure, testing, versioning.
- **Microsoft Foundry Agent Service** — https://learn.microsoft.com/en-us/azure/ai-foundry/agents/overview
    - Usage: Enterprise agents: model, instructions, tools, identity, observability, lifecycle, publishing.
- **Microsoft Foundry Tool Catalog
  ** — https://learn.microsoft.com/en-us/azure/ai-foundry/agents/concepts/tool-catalog?view=foundry
    - Usage: Built-in and custom tools, MCP, OpenAPI, A2A, authentication.
- **Model Context Protocol Specification** — https://modelcontextprotocol.io/specification/2025-11-25
    - Usage: MCP as standardized integration for LLM applications, external context, and tools.
- **MCP Sampling** — https://modelcontextprotocol.io/specification/2025-11-25/client/sampling
    - Usage: Client-controlled sampling, tool-use capability negotiation, human review recommendations.
- **A2A Specification** — https://google-a2a.github.io/A2A/specification/
    - Usage: Agent discovery, AgentCard, task lifecycle, messages, artifacts, JSON-RPC over HTTP/SSE.

## Internal Andromeda sources

- `01.Vision_Doctrine_Invariants.md`
- `02.Lexique_Andromeda_SQL_Equivalences.md`
- `03.Architecture_Globale_Modules.md`
- `04.Catalogue_Bases_Namespaces_Objets.md`
- `05.SRPL_Langage_Procedures.md`
- `06.Type_System_Tables_Enums_StructuredObjects.md`
- `07.Maps_Analytique_Temps_Reel.md`
- `08.Transaction_Kernel_WAL_MVCC_Recovery.md`
- `09.Storage_Engine_HotCold_BinaryFormats.md`
- `10.Network_QUIC_RPC_Contracts.md`
- `11.Procedure_Store_Optimizer_Stats_Benchmark.md`
- `12.Security_IAM_Admin_Audit.md`
- `13.HADR_Backup_Restore_Forensic.md`
- `14.Hardware_Rust_CPU_GPU_NVMe.md`
- `15.Modelization_Import_Batch_Catalog_Evolution.md`
- `16.Roadmap_V0_Risques_Decisions.md`
- `17.Sources_References.md`
- `Andromeda_Storage_Engine.md`
- `Fondation de SRPL pour un langage procédural relationnel strict.pdf`
- `Rapport consolidé sur les SGBDR, ACID, les transactions, l’algèbre relationnelle, la normalisation, .pdf`
- `Corpus de référence pour les SGBDR, ACID, l’algèbre relationnelle, la normalisation, les statistique.pdf`
- `Andromeda_SGBDRT_SRPL_Master_Consolidation_2026.pdf`

## Synthesis

The pack applies the following source-backed conclusions:

- Prompts and instructions must be clear, specific, structured, and testable.
- Agents require explicit goals, constraints, tools, handoff rules, and output gates.
- Skills must be focused, discoverable, versioned, and loaded only when relevant.
- Hooks must guard lifecycle boundaries such as prompt submission, tool execution, subagent completion, and final stop.
- Tooling must follow least privilege and must support tracing, audit, and replay.
- Prompt-injection defense is not only string filtering. Treat it as an adversarial social-engineering problem and
  design layered controls.
- Enterprise agent workflows require identity, RBAC, observability, versioning, evaluations, and controlled publishing.
