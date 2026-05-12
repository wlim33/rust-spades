# Available Pronounceable 5/6-Letter Crate Names — Design

**Date:** 2026-05-11
**Goal:** Produce a ranked list of pronounceable 5- and 6-letter `[a-z]+` strings not currently registered on crates.io, to help pick a name for a future crate.

## Inputs

- **Taken set:** crates.io database dump (`https://static.crates.io/db-dump.tar.gz`), `data/crates.csv`, `name` column. Filter to names matching `^[a-z]{5,6}$`, lowercased.
- **Candidate space:** all `[a-z]{5}` ∪ `[a-z]{6}` strings — 11,881,376 + 308,915,776 ≈ 320M.
- **Bigram table:** built-in 28×28 (a-z + start `^` + end `$`) English letter-pair log-probabilities. Hard-coded constant in the script, no external corpus download.

## Pipeline

1. **Fetch dump.** `curl -L https://static.crates.io/db-dump.tar.gz -o tmp/crate-names/db-dump.tar.gz`. Cached; only re-download if missing.
2. **Extract.** `tar xzf db-dump.tar.gz` into `tmp/crate-names/dump/`. Locate `data/crates.csv` inside the dated subdirectory.
3. **Build taken set.** Stream `crates.csv`, extract `name` column, lowercase, keep those matching `^[a-z]{5,6}$`. Store in a Python `set`.
4. **Score candidates.** For each length L ∈ {5, 6}:
   - Iterate all `[a-z]^L` strings.
   - Compute score = mean over bigrams `(^, c_0), (c_0, c_1), …, (c_{L-1}, $)` of log-probability.
   - Skip names in taken set.
   - Maintain a top-K heap (K = 10,000 per length).
5. **Output.** Write `tmp/crate-names/available_5.txt` and `available_6.txt`, each line `<score>\t<name>`, sorted descending by score. Print top 200 of each to stdout.

## Bigram model

- Source: a published English letter-bigram frequency table (Norvig / Google Books n-gram counts), embedded as a constant.
- Boundary tokens `^` and `$` give the model context for word-initial/final letter preferences (e.g. `q$` is near-zero, `ng$` is common).
- Score is **mean** log-prob over L+1 bigrams, so 5- and 6-letter scores are directly comparable.

## Output

Two files in `tmp/crate-names/` (gitignored), TSV format `score\tname`, sorted by score descending:

- `available_5.txt` — up to 10,000 entries
- `available_6.txt` — up to 10,000 entries

Top 200 of each printed to stdout for quick scanning.

## Non-goals

- No alternate character classes (digits, `_`, `-` excluded by user request).
- No semantic filtering — purely phonotactic scoring.
- No reservation/squatting workflow — output is read-only research.
- Not committed to the repo; output is one-off.

## Risks / open questions

- **Bigram table quality:** if results look unnatural, swap the embedded table for a corpus-trained one (would require downloading a word list — defer unless needed).
- **Heap size K=10K:** if too noisy, lower; if too restrictive, raise. Easy to re-run.
- **Dump freshness:** crates.io publishes daily; we use whatever's current at run time.

## Implementation

Single Python 3 script at `tmp/crate-names/find_names.py`. Stdlib only (`csv`, `tarfile`, `urllib.request`, `heapq`, `math`, `re`). No virtualenv, no dependencies.
