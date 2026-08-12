# Releasing shunsai

Releases are cut by [release-plz](https://release-plz.dev) from
[`.github/workflows/release-plz.yml`](./.github/workflows/release-plz.yml), which reads
the Conventional Commit prefixes described in [CLAUDE.md](./CLAUDE.md).

Two parts: a **one-time bootstrap** that only applies to v0.1.0, and the **standing
procedure** for every release after it. §1 stops being instructions once it is done, but
§1.3 does not — it is the only written record of settings that live on crates.io, which
nothing in this repository can show you or check.

---

## 1. One-time bootstrap for v0.1.0

crates.io will not accept a trusted-publishing upload for a crate that does not exist
yet — a trusted publisher configuration can only be created against an already-published
crate. So the **first upload is manual**, and everything after it is not.

### 1.1 Secrets, before merging the workflow

One repository secret, `RELEASE_PLZ_TOKEN` (Settings → Secrets and variables → Actions):
a fine-grained PAT, or a GitHub App token, with **contents: write** and **pull requests:
write** on this repository.

⚠️ **It cannot be the default `GITHUB_TOKEN`.** A pull request opened with `GITHUB_TOKEN`
triggers no workflow runs, and `main`'s ruleset requires the `check` and `msrv` contexts
with no bypass actors — so the release PR would carry no CI, satisfy no required check,
and be unmergeable by anyone, with no override available.

There is **no `CARGO_REGISTRY_TOKEN`**, and there should not be: §1.3 replaces it.

### 1.2 Publish 0.1.0 by hand

Merge the PR that adds this workflow first, then work from a clean checkout of `main`.

⚠️ **That merge fires one `Release` run that fails, and it is meant to.** The release job
will try to publish 0.1.0, find that the crate does not exist so no trusted publisher
configuration can apply to it, and stop. Nothing is left half-done; the next push, after
§1.3, is green. Publishing *before* the merge would avoid the red mark, but then the
tarball's `.cargo_vcs_info.json` names a branch commit that the squash-merge discards —
a permanent wrong pointer inside a published artifact, in exchange for a transient one
in the Actions log.

With `main` merged and checked out:

1. Re-run the provenance scan — [DESIGN.md](./DESIGN.md) §7 requires one before *each*
   release, because the comparison corpus moves. The procedure and the last result are in
   the 2026-08-11 entry of [DECISIONS.md](./DECISIONS.md).
2. Check `CHANGELOG.md`'s `0.1.0` heading carries **today's** date. That entry is
   hand-written — release-plz generates every later one, but it could not generate the
   first.
3. Confirm the tarball is what the record claims:

```bash
cargo publish --dry-run && cargo package --list
```

`DECISIONS.md`'s packaging entry says 21 files: `src/`, `README.md`, `CHANGELOG.md` and
the two licences. Then:

```bash
cargo publish
```

### 1.3 Configure Trusted Publishing

On the crate's crates.io page → Settings → Trusted Publishing → add a GitHub
configuration:

| field | value |
|---|---|
| Repository owner | `sugyan` |
| Repository name | `shunsai` |
| Workflow filename | `release-plz.yml` |
| Environment | *(leave empty)* |

Both of the last two are load-bearing and fail silently: the workflow file's **name** is
matched, so renaming it stops publishing until this is updated, and the environment must
be empty because the release job declares none.

From here `release-plz release` mints a short-lived crates.io token from the job's OIDC
identity — which is why the job needs `id-token: write`, and why no registry secret is
stored anywhere.

### 1.4 Verify

Push any commit to `main` and confirm the `Release` workflow is green. `release-plz
release` exits successfully with nothing to do once the version in `Cargo.toml` is
already on crates.io, so a green run here means authentication worked, not that it
published twice.

---

## 2. Every release after that

Ordinary work merges to `main` as usual. Then:

1. **release-plz opens a release PR** — the version bump, and the `CHANGELOG.md` entry
   built from the commit subjects since the last tag. It updates that PR as more commits
   land, so there is no rush to merge it.
2. **Review it as a release note**, not as a diff. The version is decided by
   `cargo-semver-checks` reading the compiled API, not by the commit prefixes, so a
   mistyped prefix costs changelog quality rather than a wrong bump.
3. **Re-run the provenance scan** (§1.2 step 1) — required before *each* release.
4. **Merge it.** The `release` job publishes to crates.io, tags, and cuts a GitHub
   release.

**A release PR with no CI runs on it was opened with `GITHUB_TOKEN`** — the workflow was
edited, or the token swapped. Per §1.1 that PR can never satisfy the required checks, so
close it, restore `RELEASE_PLZ_TOKEN`, and let release-plz open a fresh one. A token that
is merely expired or under-scoped fails the job outright instead, which is the louder and
easier failure.
