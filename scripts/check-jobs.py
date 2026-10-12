#!/usr/bin/env python3
"""Finite contract test: real media, real CLI, controlled local transcriber faults."""
from pathlib import Path
import hashlib
import json
import os
import signal
import time
import shutil
import subprocess
import sys
import tempfile
import uuid

binary = Path(sys.argv[1]).resolve()
ffmpeg = shutil.which("ffmpeg")
ffprobe = shutil.which("ffprobe")
if not ffmpeg or not ffprobe:
    sys.exit("Media contract checks require ffmpeg and ffprobe on PATH. Install ffmpeg first.")
construction = Path.home() / "construction_side/voxray"
construction.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="jobs-check-", dir=construction) as temporary:
    root = Path(temporary)
    (root / "bin").mkdir()
    cli = root / "bin/voxray"
    shutil.copy2(binary, cli)
    source = root / "source.m4a"
    subprocess.run([ffmpeg, "-v", "error", "-f", "lavfi", "-i",
                    "sine=frequency=440:duration=3", "-c:a", "aac", str(source)], check=True)
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    config = root / "config.toml"
    config.write_text(f'''[default]
inbox_dir = "{root}"
calls_dir = "{root}/calls"
mode = "folder"
date_format = "%Y-%m-%d"
modules = ["communication"]
''')
    request = {"recording_id": str(uuid.uuid4()), "recording": str(source),
               "recorded_at": "2026-10-09T15:00:00+02:00", "profile": "default", "name": "Daily"}
    path = root / "request.json"
    path.write_text(json.dumps(request))

    def invoke(*args, environment=None, expect="ok"):
        result = subprocess.run([str(cli), *args, "--json", "--non-interactive"],
                                capture_output=True, text=True, env=environment, timeout=20)
        response = json.loads(result.stdout)
        assert response["status"] == expect, response
        assert (result.returncode == 0) == (expect == "ok"), result.stderr
        return response.get("data", response)

    assert invoke("profiles")[0]["id"] == "default"  # No token installed.
    accepted = invoke("jobs", "submit", "--request", str(path))
    assert invoke("jobs", "submit", "--request", str(path))["id"] == accepted["id"]
    assert Path(accepted["recording"]).parent.name == "2026-10-09 Daily"
    video = root / "source.mov"
    subprocess.run([ffmpeg, "-v", "error", "-f", "lavfi", "-i", "color=size=64x64:duration=3",
                    "-i", str(source), "-c:v", "mpeg4", "-c:a", "copy", "-shortest", str(video)], check=True)
    guard_bin = root / "guard"
    guard_bin.mkdir()
    sentinel = root / "unexpected-transcription"
    guard = guard_bin / "mw"
    guard.write_text(f'#!/bin/sh\ntouch "{sentinel}"\nexit 1\n')
    guard.chmod(0o700)
    import_environment = dict(os.environ, PATH=str(guard_bin) + ":" + os.environ["PATH"])
    for index, media in enumerate([source, video]):
        original_digest = hashlib.sha256(media.read_bytes()).hexdigest()
        import_request = dict(request, recording_id=str(uuid.uuid4()), recording=str(media),
                              name=f"Import only {index}", transcribe=False)
        path.write_text(json.dumps(import_request))
        imported_job = invoke("jobs", "submit", "--request", str(path))
        stored = invoke("jobs", "run", "--id", imported_job["id"], environment=import_environment)
        assert stored["stage"] == "stored" and not Path(stored["transcript"]).exists(), stored
        assert not sentinel.exists(), "Import-only request invoked the transcriber"
        assert hashlib.sha256(media.read_bytes()).hexdigest() == original_digest
        assert invoke("jobs", "run", "--id", stored["id"], environment=import_environment)["stage"] == "stored"
        decoded = subprocess.run([ffprobe, "-v", "error", "-show_entries", "stream=codec_type",
                                  "-of", "json", stored["recording"]], capture_output=True, text=True, check=True)
        assert [s["codec_type"] for s in json.loads(decoded.stdout)["streams"]] == ["audio"]
    path.write_text(json.dumps(request))
    missing = dict(os.environ, PATH="/usr/bin:/bin")
    failed = invoke("jobs", "run", "--id", request["recording_id"], environment=missing)
    assert failed["stage"] == "failed" and failed["error"], failed
    imported = Path(failed["recording"])
    assert hashlib.sha256(imported.read_bytes()).hexdigest() == digest
    state = invoke("jobs", "status", "--id", request["recording_id"])
    assert state["recording"] == str(imported)
    interrupted_request = dict(request, recording_id=str(uuid.uuid4()), name="Interrupted")
    path.write_text(json.dumps(interrupted_request))
    interrupted = invoke("jobs", "submit", "--request", str(path))
    # A later configuration failure must not prevent completing an already accepted job.
    config.write_text("damaged config")
    fake = root / "mw"
    fake.write_text('''#!/usr/bin/env python3
import json,sys,os,time
time.sleep(float(os.environ.get("VOXRAY_TEST_DELAY", "0")))
output=sys.argv[sys.argv.index("--output")+1]
with open(output,"w") as file:
 json.dump({"segments":[{"start":0,"end":3,"speaker":"speaker_1","text":"Both voices remain in the saved recording."}]},file)
''')
    fake.chmod(0o700)
    environment = dict(os.environ, PATH=str(root) + ":/usr/bin:/bin")
    delayed = dict(environment, VOXRAY_TEST_DELAY="2")
    worker = subprocess.Popen([str(cli), "jobs", "run", "--id", interrupted_request["recording_id"],
                               "--json", "--non-interactive"], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True, env=delayed)
    try:
        deadline = time.monotonic() + 10
        state_path = root / "jobs" / interrupted_request["recording_id"] / "job.json"
        while json.loads(state_path.read_text())["stage"] != "transcribing":
            assert worker.poll() is None and time.monotonic() < deadline
            time.sleep(0.01)
        os.kill(worker.pid, signal.SIGKILL)
        # Drain inherited pipes as well, following the finite transcriber to exit.
        worker.communicate(timeout=5)
        resumed = invoke("jobs", "run", "--id", interrupted_request["recording_id"], environment=environment)
        assert resumed["stage"] == "ready", resumed
        assert hashlib.sha256(Path(resumed["recording"]).read_bytes()).hexdigest() == digest
    finally:
        if worker.poll() is None:
            worker.kill()
            worker.communicate(timeout=5)
    ready = invoke("jobs", "run", "--id", request["recording_id"], environment=environment)
    assert ready["stage"] == "ready", ready
    transcript = Path(ready["transcript"])
    transcript_digest = hashlib.sha256(transcript.read_bytes()).hexdigest()
    invoke("jobs", "run", "--id", request["recording_id"], environment=missing)
    assert hashlib.sha256(transcript.read_bytes()).hexdigest() == transcript_digest
    assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
    media = subprocess.run([ffprobe, "-v", "error", "-show_entries",
                            "format=duration", "-of", "json", str(imported)],
                           capture_output=True, text=True, check=True)
    assert float(json.loads(media.stdout)["format"]["duration"]) >= 3
    job_state = root / "jobs" / request["recording_id"] / "job.json"
    job_state.write_text("broken state")
    invoke("jobs", "run", "--id", request["recording_id"], expect="error")
    assert job_state.read_text() == "broken state"
    assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
    print("Jobs contract passed: missing transcriber, SIGKILL during transcription, retry with bad config, idempotence, readable media, corrupt-state preservation")
