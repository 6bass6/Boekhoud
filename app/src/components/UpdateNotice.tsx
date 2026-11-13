/// <reference types="vite/client" />
import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "../api";

type Status =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "found"; update: Update }
  | { kind: "busy"; text: string }
  | { kind: "error"; text: string };

/**
 * Update check and install. With `auto` it checks once on start and only shows something when an
 * update exists (errors stay silent, e.g. offline). Installing always needs a click and first makes a
 * verified backup; if that backup fails, nothing is installed. The administration itself lives in the
 * data folder, which the installer never touches.
 */
export function UpdateNotice({ auto = false, canBackup = true }: { auto?: boolean; canBackup?: boolean }) {
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  const run = async (silent: boolean) => {
    if (!silent) setStatus({ kind: "checking" });
    try {
      const update = await check();
      setStatus(update ? { kind: "found", update } : silent ? { kind: "idle" } : { kind: "none" });
    } catch (e) {
      setStatus(silent ? { kind: "idle" } : { kind: "error", text: `Controleren mislukt: ${String(e)}` });
    }
  };

  // Dev builds skip the automatic check: they would offer the released version.
  useEffect(() => { if (auto && !import.meta.env.DEV) void run(true); }, [auto]);

  const install = async (update: Update) => {
    try {
      if (canBackup) {
        setStatus({ kind: "busy", text: "Back-up maken…" });
        await api.backupNow();
      }
    } catch (e) {
      setStatus({ kind: "error", text: `Bijwerken gestopt, want de back-up is mislukt: ${String(e)}` });
      return;
    }
    try {
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        if (ev.event === "Progress") done += ev.data.chunkLength;
        const pct = total ? ` ${Math.floor((done * 100) / total)}%` : "";
        setStatus({ kind: "busy", text: ev.event === "Finished" ? "Installeren…" : `Downloaden…${pct}` });
      });
      await relaunch();
    } catch (e) {
      setStatus({ kind: "error", text: `Bijwerken mislukt: ${String(e)}. De administratie is niet aangepast.` });
    }
  };

  if (auto && status.kind === "idle") return null;
  return (
    <div className={auto ? "box warn-box" : ""}>
      {status.kind === "found" && (
        <>
          <strong>Nieuwe versie beschikbaar: {status.update.version}</strong>
          {status.update.body && <p className="small" style={{ whiteSpace: "pre-wrap" }}>{status.update.body}</p>}
          <p className="small muted">
            {canBackup ? "Eerst wordt een gecontroleerde back-up gemaakt. " : ""}
            De app sluit tijdens het installeren en start daarna opnieuw; je administratie blijft staan.
          </p>
          <div><button onClick={() => install(status.update)}>Bijwerken</button></div>
        </>
      )}
      {status.kind === "busy" && <p>{status.text}</p>}
      {status.kind === "checking" && <p className="muted">Controleren…</p>}
      {status.kind === "none" && <p className="ok">Je gebruikt de nieuwste versie.</p>}
      {status.kind === "error" && <p className="error">{status.text}</p>}
      {!auto && status.kind !== "busy" && status.kind !== "found" && (
        <div><button onClick={() => run(false)}>Controleren op updates</button></div>
      )}
    </div>
  );
}
