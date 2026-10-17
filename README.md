# timegit

> A Git wrapper for making backdated and future-dated commits easy.

`timegit` is a small wrapper around **Git** that makes it easy to create or amend commits at arbitrary points in time.

Git already supports commits with custom timestamps, but setting those timestamps manually—especially when creating multiple commits relative to some reference date—is cumbersome.

`timegit` solves that by providing a simple CLI for temporal Git operations.

You still use Git.

Your repository is still a normal Git repository.

Your commits are still normal Git commits.

`timegit` simply handles the time manipulation for you.

---

# Why timegit?

Suppose you want to make a commit dated:

```text
January 1, 2024
```

Normally, you'd need to deal with Git's environment variables and manually construct the appropriate timestamps.

With `timegit`:

```bash
timegit new "2024-01-01T12:00:00Z" -m "Project started"
```

That's it.

You can also create commits relative to a named point:

```bash
timegit new --pointer project-start +2m -m "First prototype"
```

This is particularly useful when constructing a sequence of historical, backdated, or future-dated commits.

---

# What timegit actually does

`timegit` **does not replace Git**.

It does not maintain its own commit database.

It does not create a separate version-control system.

It does not replace `.git`.

Instead, it acts as a layer on top of Git:

```text
             Your command
                  │
                  ▼
              timegit
                  │
        calculate requested time
                  │
                  ▼
             invoke Git
                  │
                  ▼
          normal Git commit
```

You can continue using:

```bash
git add
git commit
git status
git log
git checkout
git switch
git merge
git rebase
git push
```

exactly as you normally would.

`timegit` only helps when you need control over commit time.

---

# Core idea

There are two important concepts:

## Commits

These are **Git commits**.

`timegit` does not invent its own commit representation.

When you run:

```bash
timegit new ...
```

the result is an ordinary Git commit that can be inspected with:

```bash
git log
```

---

## Pointers

A pointer is simply a **named timestamp reference** stored by `timegit`.

For example:

```text
project-start
      │
      ▼
2026-01-01T00:00:00Z
```

You can then perform Git operations relative to that point:

```bash
timegit new --pointer project-start +2m -m "First prototype"
```

which means:

```text
project-start
      │
      │ +2 months
      ▼
2026-03-01T00:00:00Z
      │
      ▼
git commit
```

The pointer does not move when you use it.

---

# Installation

Build with Cargo:

```bash
cargo build --release
```

Then place the binary somewhere on your `PATH`:

```bash
cp target/release/timegit ~/.local/bin/
```

Verify:

```bash
timegit --version
```

---

# Creating backdated commits

The simplest use case is creating a commit at a specific time.

```bash
timegit new "2024-01-01T12:00:00Z" -m "Project started"
```

`timegit` calculates the timestamp and invokes Git to create the commit.

Afterward:

```bash
git log
```

will show the resulting commit as a normal Git commit with that date.

---

# Creating future-dated commits

The same mechanism works for dates in the future.

```bash
timegit new "2030-01-01T12:00:00Z" -m "Future event"
```

This creates a normal Git commit whose timestamp is in the future.

---

# Amend an existing commit

You can change the timestamp of an existing Git commit:

```bash
timegit amend <commit-hash> "2024-01-01T12:00:00Z"
```

For example:

```bash
timegit amend a31f9c2 "2024-05-10T18:00:00Z"
```

This is effectively a Git amend/history-rewrite operation performed by `timegit`.

Because Git commits are immutable objects, the underlying Git operation will create the appropriate rewritten commit history.

---

## Changing the commit message

The `-m` argument is optional when amending.

Without `-m`:

```bash
timegit amend a31f9c2 "2024-05-10T18:00:00Z"
```

the message is kept unchanged.

With `-m`:

```bash
timegit amend a31f9c2 "2024-05-10T18:00:00Z" -m "Corrected commit"
```

the commit message is changed as well.

---

# Why pointers?

Absolute timestamps are useful, but repeatedly writing full timestamps is annoying.

Suppose you want:

```text
Project started
      │
      ├── +1 month → Prototype
      ├── +3 months → Beta
      ├── +6 months → Release
      └── +1 year → Version 2
```

You could manually calculate each date.

Or define one reference point:

```bash
timegit pointer new project-start "2026-01-01T00:00:00Z"
```

Then:

```bash
timegit new --pointer project-start +1m -m "Prototype"
timegit new --pointer project-start +3m -m "Beta"
timegit new --pointer project-start +6m -m "Release"
timegit new --pointer project-start +1y -m "Version 2"
```

This is the primary reason pointers exist.

---

# Pointer commands

## Create a pointer

Create a pointer from a datetime:

```bash
timegit pointer new <pointer-name> <datetime>
```

Example:

```bash
timegit pointer new project-start "2026-01-01T00:00:00Z"
```

The pointer is persisted so it can be used across different invocations of `timegit`.

---

## Create a pointer from a Git commit

A pointer can also reference an existing Git commit:

```bash
timegit pointer new <pointer-name> --from-commit <commit-hash>
```

Example:

```bash
timegit pointer new milestone --from-commit a31f9c2
```

Now the pointer resolves to the timestamp of that Git commit.

Conceptually:

```text
milestone
    │
    ▼
Git commit a31f9c2
    │
    ▼
commit timestamp
```

This is especially useful when working on an existing Git repository.

---

# Using a pointer

A pointer can be used as the reference point for a new commit:

```bash
timegit new --pointer <pointer-name> <offset> -m "message"
```

For example:

```bash
timegit new --pointer milestone +2d -m "Two days later"
```

This does **not** move `milestone`.

It means:

```text
milestone
    │
    │ +2 days
    ▼
new timestamp
    │
    ▼
git commit
```

The pointer remains exactly where it was.

---

# Moving a pointer

Unlike using a pointer, moving a pointer actually changes it.

```bash
timegit pointer move <pointer-name> <offset>
```

Examples:

```bash
timegit pointer move project-start +2d
timegit pointer move project-start -3h
timegit pointer move project-start +30min
timegit pointer move project-start +2m
timegit pointer move project-start +1y
```

For example:

```text
project-start
      │
      ▼
2026-01-01
```

Run:

```bash
timegit pointer move project-start +2d
```

Now:

```text
project-start
      │
      ▼
2026-01-03
```

The pointer itself has moved.

---

# Deleting a pointer

Delete a pointer with:

```bash
timegit pointer delete <pointer-name>
```

Example:

```bash
timegit pointer delete project-start
```

Deleting a pointer has no effect on Git commits.

It only removes the time reference maintained by `timegit`.

---

# Listing pointers

See all available pointers:

```bash
timegit pointer list
```

Example:

```text
project-start    2026-01-01T00:00:00Z
milestone        2026-06-15T18:30:00Z
release          2026-09-01T00:00:00Z
```

---

# Showing a pointer

```bash
timegit pointer show project-start
```

This displays the pointer's current resolved timestamp and, when applicable, the commit it references.

---

# Offsets

Offsets allow you to describe a time relative to a pointer.

The syntax is:

```text
[+|-][amount][unit]
```

Supported units:

| Unit  | Meaning | Example  |
| ----- | ------- | -------- |
| `s`   | seconds | `+30s`   |
| `min` | minutes | `+20min` |
| `h`   | hours   | `-4h`    |
| `d`   | days    | `+7d`    |
| `m`   | months  | `+2m`    |
| `y`   | years   | `-1y`    |

Examples:

```text
+30s
-15min
+2h
-7d
+3m
+1y
```

The important distinction is:

```text
min = minutes
m   = months
```

So:

```bash
timegit new --pointer start +15min -m "15 minutes later"
```

means fifteen minutes.

While:

```bash
timegit new --pointer start +1m -m "One month later"
```

means one calendar month.

---

# Calendar arithmetic

Months and years are treated as calendar units rather than fixed numbers of seconds.

For example:

```text
2026-01-15 +1m
```

becomes:

```text
2026-02-15
```

But if the resulting calendar date does not exist, `timegit` reports an error.

For example:

```text
2026-01-31 +1m
```

does not silently become February 28.

Instead, the operation fails because the requested calendar date is invalid.

---

# Amending using a pointer

An existing commit can also be amended relative to a pointer.

```bash
timegit amend <commit-hash> --pointer <pointer-name> <offset>
```

For example:

```bash
timegit amend a31f9c2 --pointer project-start +2m
```

This calculates:

```text
project-start
      │
      │ +2 months
      ▼
new timestamp
      │
      ▼
amend Git commit
```

You can optionally change the message:

```bash
timegit amend a31f9c2 --pointer project-start +2m \
    -m "Updated historical event"
```

Again, `-m` is optional.

---

# Working with an existing Git repository

The intended workflow is **not** to replace Git.

Suppose you already have:

```text
my-project/
├── .git/
├── src/
└── README.md
```

You continue using Git normally:

```bash
cd my-project

git add .
git commit -m "Regular commit"
```

Then, when you need a timestamp that is not simply "now", use:

```bash
timegit new "2022-05-01T10:00:00Z" -m "Historical commit"
```

Your repository remains a normal Git repository.

You can immediately inspect the result with:

```bash
git log --format=fuller
```

---

# A practical example

Imagine you are reconstructing the history of a project.

You know the project started on:

```text
2025-01-01
```

Create a pointer:

```bash
timegit pointer new project-start "2025-01-01T00:00:00Z"
```

Now create historical commits:

```bash
timegit new --pointer project-start +0d -m "Project started"
timegit new --pointer project-start +1m -m "First prototype"
timegit new --pointer project-start +3m -m "Beta release"
timegit new --pointer project-start +6m -m "Public release"
```

Git now contains those commits with their corresponding timestamps.

You can still use:

```bash
git log
```

```bash
git show <commit>
```

```bash
git push
```

and every other normal Git command.

---

# Commit-backed pointers

Suppose the repository already contains:

```text
a31f9c2   Initial prototype
```

Create a pointer:

```bash
timegit pointer new prototype --from-commit a31f9c2
```

Now you can work relative to that Git commit:

```bash
timegit new --pointer prototype +2d -m "Two days after prototype"
```

The pointer references the Git commit itself rather than merely copying its timestamp.

This means the pointer continues to resolve through history rewrites until it is explicitly moved.

---

# Storage

`timegit` stores its own small amount of metadata under:

```text
~/.timegit/
```

This is separate from the repository's `.git` directory.

The purpose of `~/.timegit` is primarily to persist pointer information.

Git itself remains responsible for:

```text
commits
branches
objects
history
repository state
```

`timegit` remains responsible for:

```text
time calculation
offsets
named time pointers
Git command orchestration
```

---

# What timegit is not

`timegit` is not:

- a replacement for Git
- a new version-control system
- a separate commit database
- an alternative Git repository format

It is a **Git wrapper focused on time manipulation**.

The easiest way to think about it is:

```text
Git
+
easy arbitrary timestamps
+
named temporal references
=
timegit
```

---

# Command reference

## Create

```bash
timegit new <datetime> -m "message"
timegit new --pointer <pointer> <offset> -m "message"
```

## Amend

```bash
timegit amend <commit> <datetime>
timegit amend <commit> <datetime> -m "message"
timegit amend <commit> --pointer <pointer> <offset>
timegit amend <commit> --pointer <pointer> <offset> -m "message"
```

## Pointers

```bash
timegit pointer new <name> <datetime>
timegit pointer new <name> --from-commit <commit>

timegit pointer move <name> <offset>

timegit pointer delete <name>

timegit pointer list

timegit pointer show <name>
```

---

# In one sentence

**`timegit` lets Git users create and rewrite commits at arbitrary past or future times without having to manually deal with Git's timestamp environment variables and date arithmetic.**

git --no-pager log --oneline
timegit amend <hash> "2025-07-31T12:00:00Z"
git push --force origin main
