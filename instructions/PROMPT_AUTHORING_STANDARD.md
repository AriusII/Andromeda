# Prompt Authoring Standard

## Purpose

Define how prompts are written for Andromeda work.

## Requirements

A project prompt must include:

- Task objective.
- Workstream.
- Relevant source files.
- Output format.
- Non-goals.
- Invariants to preserve.
- Required verification.
- Allowed assumptions.
- Required language and style.

## Do

- Put instructions before context.
- Use clear delimiters for source material.
- Request a specific output structure.
- Specify whether research is required.
- Ask the model to state uncertainty.
- Provide examples only when they are short and representative.

## Do not

- Ask for broad autonomous behavior without boundaries.
- Mix untrusted source content with instructions.
- Use vague quality terms without acceptance criteria.
- Ask the model to hide uncertainty.
