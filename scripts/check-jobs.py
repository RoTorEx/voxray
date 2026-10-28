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
[google_drive]
folder_id = "callsFolder"
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
        assert (result.returncode == 0) == (expect in ("ok", "success")), result.stderr
        return response.get("data", response)

    discovered = invoke("profiles")[0]  # No token installed.
    assert discovered["id"] == "default" and discovered["drive_folder"] == "callsFolder"
    assert not discovered["google_drive"]
    accepted = invoke("jobs", "submit", "--request", str(path))
    assert accepted["drive"]["folder_id"] == "callsFolder"
    routed = dict(request, recording_id=str(uuid.uuid4()), drive_folder="overrideFolder")
    path.write_text(json.dumps(routed))
    assert invoke("jobs", "submit", "--request", str(path))["drive"]["folder_id"] == "overrideFolder"
    path.write_text(json.dumps(request))
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
    for index, media in enumerate([source, video, video]):
        original_digest = hashlib.sha256(media.read_bytes()).hexdigest()
        import_request = dict(request, recording_id=str(uuid.uuid4()), recording=str(media),
                              name=f"Import only {index}", transcribe=False, keep_video=index == 2)
        path.write_text(json.dumps(import_request))
        imported_job = invoke("jobs", "submit", "--request", str(path))
        stored = invoke("jobs", "run", "--id", imported_job["id"], environment=import_environment)
        assert stored["stage"] == "stored" and not Path(stored["transcript"]).exists(), stored
        retained_video = Path(stored["recording"]).parent / "video.mov"
        assert retained_video.exists() == (index == 2)
        if index == 2:
            assert hashlib.sha256(retained_video.read_bytes()).hexdigest() == original_digest
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
    # New choices are frozen in accepted jobs; missing credentials never discard audio.
    config.write_text(f'''[default]
inbox_dir = "{root}"
calls_dir = "{root}/calls"
modules = []
[default.drive]
enabled = false
folder_id = "projectFolder"
''')
    # An explicit media-only archive remains valid without invoking transcription.
    archive_request = dict(request, recording_id=str(uuid.uuid4()), name="Media only archive",
                           transcribe=False, google_drive=True)
    path.write_text(json.dumps(archive_request))
    archive = invoke("jobs", "submit", "--request", str(path))
    archived = invoke("jobs", "run", "--id", archive["id"], environment=missing)
    assert archived["stage"] == "failed" and "credentials" in archived["error"].lower(), archived
    assert Path(archived["recording"]).is_file()
    assert not Path(archived["transcript"]).exists()
    assert not archived["media_removed"]
    for upload in [False, True]:
        cleanup_request = dict(request, recording_id=str(uuid.uuid4()), name=f"Transcript only {upload}",
                               transcript_only=True, google_drive=upload)
        path.write_text(json.dumps(cleanup_request))
        cleanup = invoke("jobs", "submit", "--request", str(path))
        result = invoke("jobs", "run", "--id", cleanup["id"], environment=environment)
        assert Path(result["transcript"]).is_file()
        assert Path(result["recording"]).exists() == upload, result
        assert result["media_removed"] == (not upload)
        assert result["stage"] == ("failed" if upload else "ready"), result
        if upload:
            assert "credentials" in result["error"].lower()
            assert result["drive"]["folder_id"] == "projectFolder"
        else:
            assert invoke("jobs", "run", "--id", cleanup["id"], environment=missing)["stage"] == "ready"
            assert not list((root / "jobs" / cleanup["id"]).glob("source.*"))
            # A saved ready flag alone must not authorize recorder cleanup.
            Path(result["transcript"]).write_text("modified after verification")
            invoke("jobs", "status", "--id", cleanup["id"], expect="error")
        assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
    invalid = dict(request, recording_id=str(uuid.uuid4()), transcribe=False, transcript_only=True)
    path.write_text(json.dumps(invalid))
    invoke("jobs", "submit", "--request", str(path), expect="error")
    # Exercise the public single-stage CLI and its explicit destructive choice.
    standalone = root / "standalone.wav"
    shutil.copy2(source, standalone)
    invoke("transcribe", "--profile", "default", "--recording", str(standalone),
           "--transcript-only", environment=environment, expect="success")
    assert not standalone.exists() and (root / "standalone.transcript.txt").is_file()
    empty = root / "empty.wav"
    shutil.copy2(source, empty)
    (root / "empty.transcript.txt").write_text("")
    invoke("transcribe", "--profile", "default", "--recording", str(empty),
           "--transcript-only", environment=environment, expect="error")
    assert empty.is_file()
    print("Jobs contract passed: interruption/retry, readable media, corruption preservation, explicit transcript-only cleanup, missing-credentials retention, CLI deletion guard")
