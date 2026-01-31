# Git Notes Storage

whogitit stores attribution data using git's built-in notes feature. This page explains how notes work and how whogitit uses them.

## What Are Git Notes?

Git notes are metadata attached to commits without modifying the commits themselves. They're stored in a separate ref namespace and can be pushed/fetched independently.

Key characteristics:
- **Non-invasive**: Don't modify commit SHAs
- **Portable**: Can be shared via push/fetch
- **Flexible**: Any data can be attached
- **Native**: Built into git, no external dependencies

## whogitit's Notes Ref

Attribution is stored under `refs/notes/whogitit`:

```bash
# View notes ref
git show-ref | grep whogitit
# abc123def456... refs/notes/whogitit

# List all notes
git notes --ref=whogitit list
# abc123... def456...   (note hash, commit hash)
# ghi789... jkl012...
```

## Viewing Notes

### Raw Note Content

```bash
# View note for HEAD
git notes --ref=whogitit show HEAD

# View note for specific commit
git notes --ref=whogitit show abc123

# Pretty-print JSON
git notes --ref=whogitit show HEAD | jq .
```

### Using whogitit

```bash
# Formatted view
whogitit show HEAD

# JSON output
whogitit show --format json HEAD
```

## How Notes Are Created

### Automatic (Post-Commit Hook)

The post-commit hook creates notes automatically:

```bash
# In .git/hooks/post-commit
whogitit post-commit
```

This:
1. Reads the pending buffer
2. Analyzes changes
3. Creates the note

### Manual (Advanced)

```bash
# Add a note manually
git notes --ref=whogitit add -m '{"schema_version":2,...}' abc123

# Edit existing note
git notes --ref=whogitit edit abc123

# Remove a note
git notes --ref=whogitit remove abc123

# Copy a note
git notes --ref=whogitit copy abc123 def456
```

## Pushing Notes

Notes must be pushed separately from branches:

### Automatic (Pre-Push Hook)

After `whogitit init`, notes are pushed automatically:

```bash
git push  # Triggers pre-push hook
# Hook runs: git push origin refs/notes/whogitit
```

### Manual

```bash
git push origin refs/notes/whogitit
```

### Force Push (Caution)

If notes diverge:

```bash
git push origin refs/notes/whogitit --force
```

## Fetching Notes

### Automatic Configuration

`whogitit init` configures automatic fetching:

```bash
# Check configuration
git config --get-all remote.origin.fetch | grep notes
# +refs/notes/whogitit:refs/notes/whogitit
```

With this config, `git fetch` includes notes.

### Manual

```bash
# Fetch notes explicitly
git fetch origin refs/notes/whogitit:refs/notes/whogitit

# Fetch all notes
git fetch origin 'refs/notes/*:refs/notes/*'
```

## Notes and Rebasing

When rebasing, commits get new SHAs. Notes don't automatically follow.

### After Rebase

```bash
# Option 1: Copy notes from old to new commits
git notes --ref=whogitit copy <old-sha> <new-sha>

# Option 2: Accept loss (new commits won't have attribution)
```

### Preserving Notes During Rebase

```bash
# Before rebase, save note mappings
git log --format='%H' main..HEAD > /tmp/old-commits

# After rebase
git log --format='%H' main..HEAD > /tmp/new-commits

# Copy notes (script needed for automation)
paste /tmp/old-commits /tmp/new-commits | while read old new; do
  git notes --ref=whogitit copy "$old" "$new" 2>/dev/null || true
done
```

## Notes and Merge/Squash

### Merge Commits

Merge commits don't automatically get notes. The individual commits retain their notes.

### Squash Merging

Squash creates a new commit, losing individual notes. Options:

1. Use regular merge to preserve notes
2. Manually create a summary note for the squash
3. Accept that the PR comment serves as record

## Storage Details

### Notes Tree Structure

```
refs/notes/whogitit/
├── ab/
│   ├── c123...  → note for commit abc123...
│   └── d456...  → note for commit abd456...
├── cd/
│   └── e789...  → note for commit cde789...
...
```

Notes are stored as blobs in the git object database.

### Note Size

Each note is a JSON blob. Typical sizes:
- Small commit (1 file, 20 lines): ~500 bytes
- Medium commit (5 files, 100 lines): ~2-5 KB
- Large commit (20 files, 500 lines): ~10-20 KB

### Storage Efficiency

Git compresses notes like any other objects. Similar notes compress well together.

## Garbage Collection

Notes are git objects and subject to garbage collection:

```bash
# Notes blobs without refs will be collected
git gc

# Prune unreferenced notes
git gc --prune=now
```

Notes attached to existing commits are protected.

## Backup and Recovery

### Backup Notes

```bash
# Clone just the notes
git clone --bare <repo> --single-branch --branch refs/notes/whogitit notes-backup

# Or export
whogitit export --full-prompts -o backup.json
```

### Restore Notes

```bash
# Push notes from backup
cd notes-backup
git push <repo> refs/notes/whogitit:refs/notes/whogitit
```

## Troubleshooting

### Notes Not Showing

```bash
# Check if notes exist locally
git notes --ref=whogitit list

# If empty, fetch
git fetch origin refs/notes/whogitit:refs/notes/whogitit

# Check remote
git ls-remote origin refs/notes/whogitit
```

### Notes Diverged

```bash
# See what's different
git log refs/notes/whogitit..origin/refs/notes/whogitit

# Pull remote (may need merge)
git fetch origin refs/notes/whogitit
git update-ref refs/notes/whogitit FETCH_HEAD
```

### Corrupted Note

```bash
# Remove and recreate
git notes --ref=whogitit remove <commit>
# Pending buffer is gone, so attribution is lost for that commit
```

## See Also

- [Architecture](./architecture.md) - System design
- [Data Formats](./data-formats.md) - JSON schemas
- [Troubleshooting](../appendix/troubleshooting.md) - Common issues
