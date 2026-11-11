# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0]

### Security

- The automatic status refresh and background fetch do not run programs named in a repository's own configuration: `core.fsmonitor`, hooks, `ext::` remotes and the `uploadpack` of non-SSH remotes.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
