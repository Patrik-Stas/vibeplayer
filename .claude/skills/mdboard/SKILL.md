---
name: mdboard
description: Operate the markdown task board. Use at session start to scan for assigned tasks, and when creating or managing tasks, prompts, or documents.
---

## mdboard

This project uses mdboard — a markdown-based project management tool. Run `uvx mdboard` to view the web UI.

### Directory Structure

ALL mdboard data lives under `.mdboard/` in the project root. Nothing else. Here is the complete layout:

```
.mdboard/
├── port.json                          # runtime: {"port": N, "pid": P} — written by server
├── tasks/                             # TASKS — kanban board
│   ├── config.yaml                    # column definitions, board settings
│   ├── backlog/                       # column directories (one per column)
│   │   └── 001-my-task.md             # task files
│   ├── todo/
│   ├── in-progress/
│   ├── review/
│   ├── done/
│   └── comments/                      # task comments
│       └── {task_id}/                  # one dir per task
│           └── 20260210-143000-author.md
├── prompts/                           # PROMPTS — reusable templates
│   └── 001-my-prompt/                 # one dir per prompt
│       ├── current.md                 # latest version
│       └── revisions/                 # auto-created on each edit
│           ├── 001.md
│           └── 002.md
└── documents/                         # DOCUMENTS — reports, specs, research
    └── 001-my-doc/                    # one dir per document
        ├── current.md                 # latest version
        └── revisions/
            ├── 001.md
            └── 002.md
```

Key facts:
- `config.yaml` lives inside `.mdboard/tasks/`, NOT at `.mdboard/` root
- Prompts and documents are siblings of `tasks/` — all three are direct children of `.mdboard/`
- There is NO `documents/` or `prompts/` directory inside `tasks/` — they are separate
- `port.json` is the only file at the `.mdboard/` root level (auto-generated, not committed)

### Port Discovery

The server auto-assigns a port (range 10600-10700) and writes `.mdboard/port.json` on startup. To find the running instance: `cat .mdboard/port.json` → `{"port": N, "pid": P}`.

### Tasks

Tasks live in `.mdboard/tasks/{column}/{id:03d}-{slug}.md` as markdown files with YAML frontmatter.

Columns: `backlog/` → `todo/` → `in-progress/` → `review/` → `done/` (defined in `.mdboard/tasks/config.yaml`).

Task file format:
```
---
id: {number}
title: {title}
assignee: claude
tags: [{tags}]
created: YYYY-MM-DD
due: YYYY-MM-DD          # optional
branch: {branch-name}    # optional, only pick up when on this branch
completed: YYYY-MM-DD    # set when moving to done
---

## Description
{what needs to be done}

## Acceptance Criteria
- [ ] {criterion}

## Notes
{append decisions and progress here}
```

Filenames are zero-padded: `001-slug.md`, `002-slug.md`. IDs auto-increment across all columns.

#### Task workflow
1. Scan `.mdboard/tasks/backlog/`, `.mdboard/tasks/todo/`, and `.mdboard/tasks/in-progress/` for tasks where `assignee: claude`
2. If a task has `branch: X`, only pick it up when on that branch
3. Move task to in-progress: `mv .mdboard/tasks/todo/XXX.md .mdboard/tasks/in-progress/`
4. Work on it — check off `- [ ]` items in Acceptance Criteria as you go
5. Append notes under `## Notes` with what you did and decisions made
6. When complete: add `completed: YYYY-MM-DD` to frontmatter, then `mv .mdboard/tasks/in-progress/XXX.md .mdboard/tasks/done/`
7. If you discover bugs or new work, create task files in `.mdboard/tasks/backlog/`
8. Commit task file changes alongside code changes

### Prompts & Documents

Prompts and documents are revision-tracked markdown resources stored as siblings of `tasks/` under `.mdboard/`.

- **Prompts**: `.mdboard/prompts/{id:03d}-{slug}/current.md` — reusable prompt templates
- **Documents**: `.mdboard/documents/{id:03d}-{slug}/current.md` — reports, specs, research, decisions

Each resource is a directory containing `current.md` (the latest version) and a `revisions/` subdirectory with numbered snapshots (`001.md`, `002.md`, etc.) created automatically on each edit.

Resource file format:
```
---
id: {number}
title: {title}
created: YYYY-MM-DD
updated: YYYY-MM-DD
revision: {number}
tags: [{tags}]
---

{markdown content}
```

#### Creating a prompt or document
1. Pick the next available ID in the directory (check existing `{id:03d}-*` folders)
2. Create the directory: `.mdboard/prompts/{id:03d}-{slug}/` or `.mdboard/documents/{id:03d}-{slug}/`
3. Write `current.md` with frontmatter (revision: 1) and body content
4. Create `revisions/001.md` as the initial snapshot (same content as current.md)

#### When to use prompts vs documents
- **Prompts**: templates you'll reuse — code review checklists, PR templates, analysis frameworks
- **Documents**: one-off or evolving content — research findings, architecture decisions, meeting notes, reports
