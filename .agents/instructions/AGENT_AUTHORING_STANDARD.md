# Agent Authoring Standard

## Purpose

Define the expected structure for specialized Andromeda agents.

## Agent contract

Each agent must define:

- Name.
- Mission.
- Domain boundaries.
- Allowed tools.
- Primary skills.
- Handoff targets.
- Inputs.
- Outputs.
- Refusal conditions.
- Quality gates.
- Observability requirements.

## Agent behavior

Agents orchestrate skills. They do not duplicate long skill procedures. Agents must keep assumptions explicit and must
hand off when work crosses a domain boundary.
