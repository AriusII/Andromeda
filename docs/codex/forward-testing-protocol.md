# Forward-Testing Protocol

## Purpose

Validate skills, prompts, and agents against realistic tasks without leaking hidden conclusions.

## Procedure

1. Give the target agent or skill a realistic user-like task.
2. Provide raw artifacts, not the intended answer.
3. Avoid telling the tester what bug is expected.
4. Review the output for correctness, specificity, and validation behavior.
5. Revise the skill or prompt.
6. Repeat when the task is complex or fragile.

## Pass criteria

A forward test passes when another agent can use the artifact to produce a bounded, useful, validated output without hidden context.
