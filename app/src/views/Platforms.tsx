import { useEffect, useState } from "react";
import { api, fileToBase64, type PlatformReport } from "../api";
import { useLoad } from "../hooks";
import { centsToInput, formatDate, formatEuro, parseCents } from "../money";

const platformName = { apple: "Apple App Store", google: "Google Play" };

function formatAmount(cents: number, currency: string): string {
  return currency === "EUR" ? formatEuro(cents) : `${currency} ${centsToInput(cents)}`;
}

export function Platforms({ year }: { year: number }) {
  const list = useLoad(() => api.platformReports(), []);
  const products = useLoad(() => api.platformRevenueByProduct(year), [year]);
  const [selected, setSelected] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const importFile = async (file: File) => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.importPlatformFile(file.name, await fileToBase64(file));
      list.reload();
      products.reload();
      setSelected(r.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const reports = list.data ?? [];
  const selectedReport = reports.find((r) => r.id === selected) ?? null;
  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Platformen</h2>
          <label className="file-button primary-file">
            {busy ? "Importeren…" : "Rapport importeren…"}
            <input type="file" accept=".txt,.tsv,.csv" hidden onChange={(e) => { const f = e.target.files?.[0]; if (f) importFile(f); e.target.value = ""; }} />
          </label>
        </header>
        <p className="muted small">
          Apple: App Store Connect → Payments and Financial Reports → financieel rapport (alle landen, één bestand).
          Google: Play Console → Download reports → Financial → Earnings (CSV, uitgepakt).
        </p>
        {error && <p className="error">{error}</p>}
        {list.error && <p className="error">{list.error}</p>}
        {reports.length === 0 && list.data && <p className="muted">Nog geen rapporten geïmporteerd.</p>}
        {reports.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Platform</th><th>Periode</th><th>Valuta's</th><th>Status</th><th className="num">Opbrengst</th></tr></thead>
            <tbody>
              {reports.map((r) => (
                <tr key={r.id} className={selected === r.id ? "selected" : ""} onClick={() => setSelected(r.id)}>
                  <td>{platformName[r.platform]}</td>
                  <td>{formatDate(r.period_start)} – {formatDate(r.period_end)}</td>
                  <td className="desc">{r.currencies.map((c) => c.currency).join(", ")}</td>
                  <td>{r.entry_id ? <span className="tag state-paid">geboekt</span> : <span className="tag state-partially_paid">te boeken</span>}</td>
                  <td className="num">{r.proceeds_eur_cents !== null ? formatEuro(r.proceeds_eur_cents) : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {products.data && products.data.length > 0 && (
          <>
            <h3>Opbrengst per app {year} (in rapportvaluta)</h3>
            <table>
              <thead><tr><th>Platform</th><th>Product</th><th className="num">Aantal</th><th className="num">Opbrengst</th></tr></thead>
              <tbody>
                {products.data.map(([platform, product, currency, units, cents], i) => (
                  <tr key={i}><td>{platformName[platform as "apple" | "google"]}</td><td className="desc">{product || "—"}</td><td className="num">{units}</td><td className="num">{formatAmount(cents, currency)}</td></tr>
                ))}
              </tbody>
            </table>
          </>
        )}
      </div>
      {selectedReport && <ReportPanel key={selectedReport.id} report={selectedReport} onChanged={() => { list.reload(); products.reload(); }} onDeleted={() => { setSelected(null); list.reload(); products.reload(); }} />}
    </section>
  );
}

function ReportPanel({ report, onChanged, onDeleted }: { report: PlatformReport; onChanged: () => void; onDeleted: () => void }) {
  const [eur, setEur] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const init: Record<string, string> = {};
    for (const c of report.currencies) init[c.currency] = c.currency === "EUR" ? centsToInput(c.amount_cents) : "";
    setEur(init);
  }, [report]);

  const total = report.currencies.reduce((s, c) => s + (parseCents((eur[c.currency] ?? "").replace(/^-/, "")) ?? 0) * ((eur[c.currency] ?? "").trim().startsWith("-") ? -1 : 1), 0);
  const booked = report.entry_id !== null;
  const book = () =>
    api.bookPlatformReport(report.id, report.currencies.map((c) => ({ currency: c.currency, eur: eur[c.currency] ?? "" })))
      .then(onChanged).catch((e) => setError(String(e)));

  return (
    <aside className="panel">
      <h3>{platformName[report.platform]}</h3>
      <p className="small muted">{formatDate(report.period_start)} – {formatDate(report.period_end)} · {report.original_filename}</p>
      {report.overlapping.length > 0 && <p className="warning small">Let op: er is nog een rapport voor dezelfde periode. Controleer dat je niets dubbel boekt.</p>}
      <table>
        <thead><tr><th>Valuta</th><th className="num">Aantal</th><th className="num">Volgens rapport</th><th className="num">In euro</th></tr></thead>
        <tbody>
          {report.currencies.map((c) => (
            <tr key={c.currency}>
              <td>{c.currency}</td>
              <td className="num">{c.units}</td>
              <td className="num">{formatAmount(c.amount_cents, c.currency)}</td>
              <td className="num">
                {booked || c.currency === "EUR"
                  ? (c.currency === "EUR" ? formatEuro(c.amount_cents) : "")
                  : <input className="num" value={eur[c.currency] ?? ""} placeholder="0,00" onChange={(e) => setEur({ ...eur, [c.currency]: e.target.value })} />}
              </td>
            </tr>
          ))}
        </tbody>
        <tfoot><tr><td colSpan={3}>Totaal te boeken</td><td className="num">{formatEuro(booked ? report.proceeds_eur_cents ?? 0 : total)}</td></tr></tfoot>
      </table>
      {report.breakdown.types && (
        <>
          <h4>Opbouw (Google)</h4>
          <table><tbody>{report.breakdown.types.map((t) => (
            <tr key={t.transaction_type}><td>{t.transaction_type}</td><td className="num">{t.rows}×</td><td className="num">{formatEuro(t.amount_cents)}</td></tr>
          ))}</tbody></table>
        </>
      )}
      {!booked && report.platform === "apple" && report.currencies.some((c) => c.currency !== "EUR") && (
        <p className="small muted">Vul per valuta het euro-bedrag in uit App Store Connect → Payments and Financial Reports (kolom “Proceeds”). Zo sluit de boeking aan op de uitbetaling.</p>
      )}
      {error && <p className="error">{error}</p>}
      {booked
        ? <p className="ok small">Geboekt in het platformdagboek als te ontvangen bedrag. De uitbetaling koppel je via de bankimport.</p>
        : <div className="row gap">
            <button className="primary" onClick={book}>Boeken</button>
            <button onClick={() => api.deletePlatformReport(report.id).then(onDeleted).catch((e) => setError(String(e)))}>Verwijderen</button>
          </div>}
      {report.document_id && <p className="small"><a onClick={() => api.openDocument(report.document_id!)}>Origineel rapport openen</a></p>}
    </aside>
  );
}
