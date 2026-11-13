import { useEffect, useState } from "react";
import { api, type BackupInfo, type Company, type VatRegime } from "../api";
import { useLoad } from "../hooks";
import { monthNames, todayIso } from "../money";
import { UpdateNotice } from "../components/UpdateNotice";

export function Periods({ year }: { year: number }) {
  const periods = useLoad(() => api.periods(year), [year]);
  const [error, setError] = useState<string | null>(null);
  const [reopening, setReopening] = useState<number | null>(null);
  const [reason, setReason] = useState("");

  const run = (p: Promise<void>) => {
    setError(null);
    p.then(() => { setReopening(null); setReason(""); periods.reload(); }).catch((e) => setError(String(e)));
  };

  return (
    <section>
      <h2>Perioden {year}</h2>
      <p className="muted small">In een afgesloten maand kan niet meer worden geboekt. Heropenen wordt met reden vastgelegd in het auditlog.</p>
      {(error || periods.error) && <p className="error">{error ?? periods.error}</p>}
      <table>
        <tbody>
          {periods.data?.map((p) => (
            <tr key={p.month}>
              <td>{monthNames[p.month - 1]}</td>
              <td>{p.closed ? <span className="tag">afgesloten</span> : "open"}</td>
              <td>
                {!p.closed && <button onClick={() => run(api.closePeriod(year, p.month))}>Afsluiten</button>}
                {p.closed && reopening !== p.month && <button onClick={() => setReopening(p.month)}>Heropenen…</button>}
                {p.closed && reopening === p.month && (
                  <span className="row gap">
                    <input autoFocus placeholder="Reden" value={reason} onChange={(e) => setReason(e.target.value)} />
                    <button className="primary" onClick={() => run(api.reopenPeriod(year, p.month, reason))}>Heropenen</button>
                    <button onClick={() => setReopening(null)}>Annuleren</button>
                  </span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

const emptyCompany: Company = {
  iban: null, email: null, payment_term_days: 30,
  legal_name: "", trade_name: null, kvk_number: null, vat_number: null,
  address: null, postal_code: null, city: null, country: "NL",
};

export function Settings() {
  const loaded = useLoad(() => api.company(), []);
  const [company, setCompany] = useState<Company>(emptyCompany);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const today = todayIso();
  const regime = useLoad(() => api.vatRegime(today), [today]);
  const [newRegime, setNewRegime] = useState<VatRegime>("kor");
  const [regimeFrom, setRegimeFrom] = useState(`${new Date().getFullYear() + 1}-01-01`);

  useEffect(() => { if (loaded.data) setCompany(loaded.data); }, [loaded.data]);

  const field = (key: keyof Company, label: string) => (
    <label>{label}
      <input value={company[key] ?? ""} onChange={(e) => setCompany({ ...company, [key]: e.target.value })} />
    </label>
  );
  const report = (p: Promise<unknown>, ok: string) =>
    p.then(() => setMessage({ ok: true, text: ok })).catch((e) => setMessage({ ok: false, text: String(e) }));

  return (
    <section>
      <h2>Instellingen</h2>
      {message && <p className={message.ok ? "ok" : "error"}>{message.text}</p>}

      <h3>Onderneming</h3>
      <div className="form-grid">
        {field("legal_name", "Statutaire naam")}
        {field("trade_name", "Handelsnaam")}
        {field("kvk_number", "KvK-nummer")}
        {field("vat_number", "Btw-id")}
        {field("address", "Adres")}
        {field("postal_code", "Postcode")}
        {field("city", "Plaats")}
        {field("country", "Land")}
        {field("iban", "IBAN (voor op facturen)")}
        {field("email", "E-mail")}
        <label>Betaaltermijn (dagen)
          <input type="number" min={0} max={365} value={company.payment_term_days}
                 onChange={(e) => setCompany({ ...company, payment_term_days: Number(e.target.value) || 0 })} />
        </label>
      </div>
      <button className="primary" onClick={() => report(api.saveCompany(company), "Ondernemingsgegevens opgeslagen.")}>Opslaan</button>

      <h3>Btw-regime</h3>
      <p>Vandaag: <strong>{regime.data === "kor" ? "Kleineondernemersregeling (KOR)" : regime.data === "regular" ? "Regulier" : "…"}</strong></p>
      <div className="form-grid">
        <label>Nieuw regime
          <select value={newRegime} onChange={(e) => setNewRegime(e.target.value as VatRegime)}>
            <option value="regular">Regulier</option>
            <option value="kor">KOR</option>
          </select>
        </label>
        <label>Met ingang van <input type="date" value={regimeFrom} onChange={(e) => setRegimeFrom(e.target.value)} /></label>
      </div>
      <button onClick={() => report(api.setVatRegime(regimeFrom, newRegime, "ingesteld via Instellingen").then(regime.reload), "Btw-regime vastgelegd.")}>
        Regime vastleggen
      </button>
      <p className="muted small">Lees eerst §5 van docs/accounting-rules.md en overleg met een boekhouder voordat je de KOR kiest.</p>

      <Backups />
    </section>
  );
}

const kindLabel: Record<BackupInfo["kind"], string> = {
  auto: "automatisch", handmatig: "handmatig", "voor-migratie": "voor update", "voor-import": "voor import", overig: "overig",
};

const formatSize = (bytes: number) =>
  bytes >= 1024 * 1024 ? `${(bytes / 1024 / 1024).toLocaleString("nl-NL", { maximumFractionDigits: 1 })} MB` : `${Math.ceil(bytes / 1024)} kB`;

function check(b: BackupInfo) {
  const v = b.verification;
  if (!v) return <span className="muted">niet gecontroleerd</span>;
  if (v.ok) return <span className="ok">in orde ({v.entries} boekingen, {v.documents} documenten)</span>;
  return <span className="error">mislukt: {v.problems.join("; ")}</span>;
}

function Backups() {
  const status = useLoad(() => api.backupStatus(), []);
  // `move` messages are shown under the export/import controls, `backup` ones under the backup heading.
  const [message, setMessage] = useState<{ ok: boolean; text: string; at: "backup" | "move" } | null>(() => {
    const text = sessionStorage.getItem("importMessage");
    return text ? { ok: true, text, at: "move" } : null;
  });
  const [busy, setBusy] = useState(false);
  const [extraDir, setExtraDir] = useState("");
  const [exportDir, setExportDir] = useState("");
  const [importDir, setImportDir] = useState("");
  useEffect(() => sessionStorage.removeItem("importMessage"), []);
  useEffect(() => { if (status.data) setExtraDir(status.data.extra_dir ?? ""); }, [status.data]);

  const run = <T,>(p: Promise<T>, ok: (v: T) => string, at: "backup" | "move" = "backup") => {
    setBusy(true);
    setMessage(null);
    p.then((v) => setMessage({ ok: true, text: ok(v), at }))
      .catch((e) => setMessage({ ok: false, text: String(e), at }))
      .finally(() => { setBusy(false); status.reload(); });
  };
  const s = status.data;
  const last = s?.last_good;

  return (
    <>
      <h3>Back-up</h3>
      {status.error && <p className="error">{status.error}</p>}
      {message?.at === "backup" && <p className={message.ok ? "ok" : "error"}>{message.text}</p>}
      <p>Laatste goede back-up:{" "}
        {last ? <strong>{last.created} ({kindLabel[last.kind]})</strong> : <strong className="error">nog geen gecontroleerde back-up</strong>}
        {s?.days_since_good != null && s.days_since_good >= 7 && <span className="error"> — ouder dan 7 dagen</span>}
      </p>
      {s?.notice && <p className="small">{s.notice}</p>}
      <button disabled={busy} onClick={() => run(api.backupNow(), () => "Back-up gemaakt en gecontroleerd.")}>Nu back-up maken</button>
      <p className="muted small">
        Bij het starten maakt de app automatisch een back-up als de laatste goede automatische back-up ouder is dan 24 uur.
        Elke back-up wordt direct gecontroleerd (database-integriteit, hashketen en documenten). De laatste 14 automatische
        back-ups blijven bewaard; handmatige back-ups en kopieën van vóór een update worden nooit verwijderd.
        Back-ups staan in {s?.backup_dir ?? "…"}. Terugzetten: zie docs/backup.md.
      </p>
      <div className="form-grid">
        <label>Extra back-upmap (bijv. USB-stick of OneDrive-map, leeg = geen)
          <input value={extraDir} placeholder="E:\" onChange={(e) => setExtraDir(e.target.value)} />
        </label>
      </div>
      <button disabled={busy} onClick={() => run(api.saveBackupSettings(extraDir), () => "Extra back-upmap opgeslagen.")}>Map opslaan</button>
      {s && s.backups.length > 0 && (
        <table>
          <thead><tr><th>Datum</th><th>Soort</th><th>Grootte</th><th>Controle</th></tr></thead>
          <tbody>
            {s.backups.map((b) => (
              <tr key={b.file} title={b.file}>
                <td>{b.created ?? b.file}</td>
                <td>{kindLabel[b.kind]}</td>
                <td>{formatSize(b.size_bytes)}</td>
                <td>{check(b)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <h3>Verhuizen naar een andere pc</h3>
      <p className="muted small">
        Exporteren schrijft een map <code>boekhoud-export-…</code> met de database, alle documenten en een lijst met controlesommen.
        Kopieer die map naar de nieuwe pc en importeer hem daar in een nog lege administratie. Zie docs/backup.md.
      </p>
      <div className="form-grid">
        <label>Doelmap voor de export (leeg = map exports in de gegevensmap)
          <input value={exportDir} onChange={(e) => setExportDir(e.target.value)} />
        </label>
      </div>
      <button disabled={busy} onClick={() => run(api.exportAdministration(exportDir), (p) => `Export gemaakt en gecontroleerd: ${p}`, "move")}>
        Administratie exporteren
      </button>
      <div className="form-grid">
        <label>Exportmap om te importeren (alleen in een lege administratie)
          <input value={importDir} placeholder="bijv. E:\boekhoud-export-20261113-090000" onChange={(e) => setImportDir(e.target.value)} />
        </label>
      </div>
      <button disabled={busy || !importDir.trim()} onClick={() => {
        setBusy(true);
        setMessage(null);
        api.importAdministration(importDir)
          // Reload so every screen (also the company form above) shows the imported administration.
          .then((m) => { sessionStorage.setItem("importMessage", m); window.location.reload(); })
          .catch((e) => { setMessage({ ok: false, text: String(e), at: "move" }); setBusy(false); });
      }}>
        Administratie importeren
      </button>
      {message?.at === "move" && <p className={message.ok ? "ok" : "error"}>{message.text}</p>}

      <h3>Updates</h3>
      <p className="muted small">
        Bij het starten kijkt de app of er een nieuwe versie is (alleen een versiebestand van GitHub, er gaan geen
        gegevens mee). Installeren gebeurt pas na je klik, na een gecontroleerde back-up.
      </p>
      <UpdateNotice />
    </>
  );
}
