"""Shared utilities for pi session diagnosis scripts."""

from __future__ import annotations

import contextlib
import json
from datetime import datetime
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import argparse

# ── Session discovery ──────────────────────────────────────────────────────────


def slug(path: Path) -> str:
    return "--" + str(path).replace("/", "-").lstrip("-") + "--"


def session_dir(project: Path) -> Path:
    return Path.home() / ".pi" / "agent" / "sessions" / slug(project)


def find_sessions(project: Path, n: int) -> list[Path]:
    sdir = session_dir(project)
    if not sdir.exists():
        return []
    dirs = sorted(
        [d for d in sdir.iterdir() if d.is_dir() and d.name != "subagent-artifacts"],
        reverse=True,
    )
    return dirs[:n]


def find_orchestrator_jsonl(session_dir: Path) -> Path | None:
    p = session_dir.parent / (session_dir.name + ".jsonl")
    return p if p.exists() else None


def find_runs(session_dir: Path) -> list[tuple[str, str, Path]]:
    """Return (run_group_id, run_name, session_jsonl_path) sorted by run index."""
    runs: list[tuple[str, str, Path]] = []
    for subdir in sorted(session_dir.iterdir()):
        if not subdir.is_dir():
            continue
        runs.extend(
            (subdir.name, run.name, run / "session.jsonl")
            for run in sorted(subdir.iterdir())
            if run.is_dir() and (run / "session.jsonl").exists()
        )
    return runs


def latest_session(project: Path) -> Path | None:
    sessions = find_sessions(project, 1)
    return sessions[0] if sessions else None


# ── JSONL parsing ──────────────────────────────────────────────────────────────


def load_jsonl(path: Path) -> list[dict]:
    lines = []
    with path.open() as f:
        for raw in f:
            stripped = raw.strip()
            if stripped:
                with contextlib.suppress(json.JSONDecodeError):
                    lines.append(json.loads(stripped))
    return lines


def agent_name(lines: list[dict]) -> str:
    return next(
        (ev.get("name", "?") for ev in lines if ev.get("type") == "session_info"), "?"
    )


# ── Message iteration ──────────────────────────────────────────────────────────


def messages(lines: list[dict]) -> list[dict]:
    """Return only message events."""
    return [ev for ev in lines if ev.get("type") == "message"]


def tool_calls(msg: dict) -> list[dict]:
    """Extract tool calls; handles pi toolCall/arguments format."""
    return [
        {
            "name": c.get("name", "?"),
            "args": c.get("arguments", c.get("input", {})),
        }
        for c in msg.get("content") or []
        if isinstance(c, dict) and c.get("type") in ("tool_use", "toolCall")
    ]


def tool_result_text(msg: dict) -> str:
    """Extract concatenated text from a toolResult message."""
    return "\n".join(
        c["text"]
        for c in msg.get("content") or []
        if isinstance(c, dict) and c.get("text")
    )


def assistant_texts(msg: dict) -> list[str]:
    """Extract non-empty text blocks from an assistant message."""
    return [
        c["text"]
        for c in (msg.get("content") or [])
        if isinstance(c, dict) and c.get("type") == "text" and c.get("text", "").strip()
    ]


# ── Token utilities ────────────────────────────────────────────────────────────


def estimate_tokens(text: str) -> int:
    """Conservative estimate: chars / 4."""
    return len(text) // 4


def message_token_estimate(msg: dict) -> int:
    total = 0
    for c in msg.get("content") or []:
        if isinstance(c, dict):
            total += len(
                c.get("text", "")
                or c.get("thinking", "")
                or json.dumps(c.get("arguments", c.get("input", "")))
            )
    return total // 4


def reported_tokens(msg: dict) -> int:
    """Tokens reported in usage field (0 if not present or zero)."""
    u = msg.get("usage", {})
    return (
        u.get("input", 0)
        + u.get("output", 0)
        + u.get("cacheRead", 0)
        + u.get("cacheWrite", 0)
    )


# ── Timestamp formatting ───────────────────────────────────────────────────────


def fmt_ts(ts: str) -> str:
    """Trim to HH:MM:SS portion."""
    return ts[-12:-4] if len(ts) >= 12 else ts


def duration_min(lines: list[dict]) -> float:
    """Duration of a session in minutes from first to last timestamped event."""
    timestamps = [ev["timestamp"] for ev in lines if ev.get("timestamp")]
    if len(timestamps) < 2:
        return 0.0
    t0 = datetime.fromisoformat(timestamps[0])
    t1 = datetime.fromisoformat(timestamps[-1])
    return (t1 - t0).total_seconds() / 60


# ── CLI helpers ────────────────────────────────────────────────────────────────


def resolve_session(args_session: str | None, project: Path) -> Path | None:
    """Resolve a session directory from a partial ID string or return the latest."""
    if args_session is None:
        return latest_session(project)
    sdir = session_dir(project)
    matches = [d for d in sdir.iterdir() if d.is_dir() and args_session in d.name]
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        # Return most recent match
        return sorted(matches, reverse=True)[0]
    return None


def add_common_args(parser: argparse.ArgumentParser) -> None:
    """Add --project-dir and --session args to an argparse parser."""
    parser.add_argument(
        "--project-dir",
        "-d",
        default=None,
        help="Project directory (default: cwd)",
    )
    parser.add_argument(
        "--session",
        "-s",
        default=None,
        help="Partial session ID or timestamp to match (default: most recent)",
    )


def common_setup(args: argparse.Namespace) -> tuple[Path, Path | None]:
    """Return (project_path, session_dir_or_None) from parsed common args."""
    project = Path(args.project_dir).resolve() if args.project_dir else Path.cwd()
    sess = resolve_session(getattr(args, "session", None), project)
    return project, sess
