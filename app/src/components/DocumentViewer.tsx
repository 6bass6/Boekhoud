import { useEffect, useState } from "react";
import { api, fileToBase64, type DocumentInfo } from "../api";

/** Shows a stored document (PDF or image) inline, with a button to open it externally. */
export function DocumentViewer({ id, height = 520 }: { id: string; height?: number }) {
  const [url, setUrl] = useState<string | null>(null);
  const [info, setInfo] = useState<DocumentInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let objectUrl: string | null = null;
    setUrl(null);
    api.documentContent(id).then(({ info, data_base64 }) => {
      const bytes = Uint8Array.from(atob(data_base64), (c) => c.charCodeAt(0));
      objectUrl = URL.createObjectURL(new Blob([bytes], { type: info.mime_type }));
      setInfo(info);
      setUrl(objectUrl);
    }).catch((e) => setError(String(e)));
    return () => { if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [id]);

  if (error) return <p className="error">{error}</p>;
  if (!url || !info) return <p className="muted">Document laden…</p>;
  return (
    <div className="doc-viewer">
      <div className="row">
        <span className="small muted">{info.original_filename} · bewaren t/m {info.retention_until}</span>
        <button onClick={() => api.openDocument(id).catch((e) => setError(String(e)))}>Openen</button>
      </div>
      {info.mime_type.startsWith("image/")
        ? <img src={url} alt={info.original_filename} style={{ maxWidth: "100%", maxHeight: height }} />
        : info.mime_type === "application/pdf"
          ? <iframe src={url} title={info.original_filename} style={{ width: "100%", height }} />
          : <p className="muted small">Geen voorbeeld beschikbaar voor dit bestandstype.</p>}
    </div>
  );
}

/** File picker that uploads to the document store and returns the stored documents (several with `multiple`). */
export function DocumentUpload({ type, onUploaded, label = "Bestand kiezen…", multiple = false }: {
  type: string; onUploaded: (docs: DocumentInfo[]) => void; label?: string; multiple?: boolean;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <span>
      <label className="file-button">
        {busy ? "Uploaden…" : label}
        <input type="file" accept=".pdf,.png,.jpg,.jpeg,.webp,.xml" multiple={multiple} hidden onChange={async (e) => {
          const files = Array.from(e.target.files ?? []);
          if (files.length === 0) return;
          setBusy(true);
          setError(null);
          const done: DocumentInfo[] = [];
          const failed: string[] = [];
          for (const file of files) {
            try {
              done.push(await api.uploadDocument(file.name, await fileToBase64(file), type));
            } catch (err) {
              failed.push(`${file.name}: ${String(err)}`);
            }
          }
          if (failed.length) setError(failed.join("; "));
          if (done.length) onUploaded(done);
          setBusy(false);
          e.target.value = "";
        }} />
      </label>
      {error && <span className="error small"> {error}</span>}
    </span>
  );
}

/** Several documents: one tab per file, the chosen one shown below. `onRemove` adds a remove button per tab. */
export function DocumentTabs({ docs, height, onRemove }: { docs: DocumentInfo[]; height?: number; onRemove?: (id: string) => void }) {
  const [active, setActive] = useState<string | null>(null);
  if (docs.length === 0) return null;
  const shown = docs.find((d) => d.id === active) ?? docs[0];
  return (
    <div>
      {docs.length > 1 || onRemove ? (
        <div className="tabs doc-tabs">
          {docs.map((d) => (
            <span key={d.id} className="row">
              <button className={d.id === shown.id ? "active" : ""} onClick={() => setActive(d.id)} title={d.original_filename}>
                {d.original_filename}
              </button>
              {onRemove && <button className="link small" title="Weghalen" onClick={() => onRemove(d.id)}>×</button>}
            </span>
          ))}
        </div>
      ) : null}
      <DocumentViewer key={shown.id} id={shown.id} height={height} />
    </div>
  );
}
