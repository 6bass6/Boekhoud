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

/** File picker that uploads to the document store and returns the stored document. */
export function DocumentUpload({ type, onUploaded, label = "Bestand kiezen…" }: { type: string; onUploaded: (d: DocumentInfo) => void; label?: string }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <span>
      <label className="file-button">
        {busy ? "Uploaden…" : label}
        <input type="file" accept=".pdf,.png,.jpg,.jpeg,.webp,.xml" hidden onChange={async (e) => {
          const file = e.target.files?.[0];
          if (!file) return;
          setBusy(true);
          setError(null);
          try {
            onUploaded(await api.uploadDocument(file.name, await fileToBase64(file), type));
          } catch (err) {
            setError(String(err));
          } finally {
            setBusy(false);
            e.target.value = "";
          }
        }} />
      </label>
      {error && <span className="error small"> {error}</span>}
    </span>
  );
}
