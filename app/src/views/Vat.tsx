import { useEffect, useState } from "react";
import { api } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro } from "../money";

function quarters(year: number) {
  return [1, 2, 3, 4].map((q) => {
    const start = `${year}-${String((q - 1) * 3 + 1).padStart(2, "0")}-01`;
    const endMonth = q * 3;
    const lastDay = new Date(Date.UTC(year, endMonth, 0)).getUTCDate();
    return { label: `Q${q} ${year}`, start, end: `${year}-${String(endMonth).padStart(2, "0")}-${lastDay}` };
  });
}

export function Vat({ year }: { year: number }) {
  const qs = quarters(year);
  const currentQ = new Date().getFullYear() === year ? Math.floor(new Date().getMonth() / 3) : 3;
  const [qi, setQi] = useState(currentQ);
  const q = qs[qi];
  const ret = useLoad(() => api.vatReturn(q.start, q.end), [q.start, q.end]);
  const icp = useLoad(() => api.icpListing(q.start, q.end), [q.start, q.end]);
  const [detail, setDetail] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);

  useEffect(() => { setDetail(null); setConfirm(false); }, [qi, year]);
  const r = ret.data;
  const rounded = (rubric: string) => r?.rounded.rubrics.find((x) => x[0] === rubric);
  const euro = (n: number) => `€ ${n.toLocaleString("nl-NL")}`;

  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Btw-aangifte</h2>
          <div className="tabs" style={{ margin: 0, border: 0 }}>
            {qs.map((x, i) => <button key={x.label} className={qi === i ? "active" : ""} onClick={() => setQi(i)}>{x.label}</button>)}
          </div>
        </header>
        <p className="muted small">{formatDate(q.start)} t/m {formatDate(q.end)}. Vul de afgeronde bedragen in bij Mijn Belastingdienst Zakelijk. Klik op een rubriek voor de onderliggende boekingen.</p>
        {ret.error && <p className="error">{ret.error}</p>}
        {r && (
          <>
            {r.warnings.length > 0 && <div className="box warn-box"><ul className="small">{r.warnings.map((w) => <li key={w}>{w}</li>)}</ul></div>}
            <table className="clickable">
              <thead><tr><th>Rubriek</th><th className="num">Omzet</th><th className="num">Btw</th><th className="num">Aangifte (afgerond)</th></tr></thead>
              <tbody>
                {r.rubrics.map((x) => {
                  const rd = rounded(x.rubric);
                  const empty = x.base_cents === 0 && x.vat_cents === 0;
                  return (
                    <tr key={x.rubric} className={(empty ? "inactive " : "") + (detail === x.rubric ? "selected" : "")} onClick={() => !empty && setDetail(x.rubric)}>
                      <td className="desc"><strong>{x.rubric}</strong> {x.label}</td>
                      <td className="num">{empty ? "" : formatEuro(x.base_cents)}</td>
                      <td className="num">{empty || x.rubric.startsWith("3") ? "" : formatEuro(x.vat_cents)}</td>
                      <td className="num">{empty ? "" : `${euro(rd?.[1] ?? 0)}${x.rubric.startsWith("3") ? "" : ` / ${euro(rd?.[2] ?? 0)}`}`}</td>
                    </tr>
                  );
                })}
              </tbody>
              <tfoot>
                <tr><td>5a Verschuldigde btw</td><td /><td className="num">{formatEuro(r.due_cents)}</td><td className="num">{euro(r.rounded.due)}</td></tr>
                <tr className="clickable-row" onClick={() => setDetail("5b")}><td>5b Voorbelasting</td><td /><td className="num">{formatEuro(r.input_vat_cents)}</td><td className="num">{euro(r.rounded.input_vat)}</td></tr>
                <tr><td><strong>{r.rounded.balance >= 0 ? "Te betalen" : "Terug te vragen"}</strong></td><td /><td className="num">{formatEuro(Math.abs(r.balance_cents))}</td><td className="num"><strong>{euro(Math.abs(r.rounded.balance))}</strong></td></tr>
              </tfoot>
            </table>
            <p className="muted small">Afronding in je voordeel: omzet en verschuldigde btw naar beneden, voorbelasting naar boven.</p>

            <h3>Opgaaf intracommunautaire prestaties (ICP)</h3>
            {icp.data && icp.data.length === 0 && <p className="muted small">Geen ICP-diensten in dit kwartaal.</p>}
            {icp.data && icp.data.length > 0 && (
              <table>
                <thead><tr><th>Btw-id afnemer</th><th>Naam</th><th>Land</th><th className="num">Diensten</th></tr></thead>
                <tbody>{icp.data.map((x) => (
                  <tr key={(x.vat_number ?? "") + x.name}><td>{x.vat_number ?? <span className="warning">ontbreekt</span>}</td><td className="desc">{x.name}</td><td>{x.country}</td><td className="num">{formatEuro(x.services_cents)}</td></tr>
                ))}</tbody>
              </table>
            )}
            <PlatformContacts onChanged={icp.reload} />

            <h3>Vastleggen</h3>
            {r.filed
              ? <p className="ok small">Vastgelegd op {r.filed.filed_at.slice(0, 10)}. De btw-rekeningen zijn overgeboekt naar 1520 en de maanden zijn afgesloten.</p>
              : confirm
                ? <div className="box">
                    <p className="small">Leg pas vast nádat je de aangifte bij de Belastingdienst hebt ingediend. De bedragen worden bevroren, de btw-rekeningen overgeboekt naar 1520 en de maanden van dit kwartaal afgesloten.</p>
                    <span className="row gap">
                      <button className="primary" onClick={() => api.fileVatReturn(q.start, q.end).then(() => { ret.reload(); setConfirm(false); }).catch((e) => setError(String(e)))}>Ja, aangifte is ingediend</button>
                      <button onClick={() => setConfirm(false)}>Annuleren</button>
                    </span>
                  </div>
                : <button onClick={() => setConfirm(true)}>Aangifte vastleggen…</button>}
            {error && <p className="error">{error}</p>}
          </>
        )}
      </div>
      {detail && <VatDetail start={q.start} end={q.end} rubric={detail} />}
    </section>
  );
}

function VatDetail({ start, end, rubric }: { start: string; end: string; rubric: string }) {
  const lines = useLoad(() => api.vatReturnDetail(start, end, rubric), [start, end, rubric]);
  return (
    <aside className="panel">
      <h3>Rubriek {rubric}</h3>
      {lines.error && <p className="error">{lines.error}</p>}
      {lines.data && (
        <table>
          <thead><tr><th>Boeking</th><th>Omschrijving</th><th className="num">Grondslag</th><th className="num">Btw</th></tr></thead>
          <tbody>{lines.data.map((l) => (
            <tr key={l.entry_id + l.tax_code}><td>{l.entry_number}<div className="muted small">{formatDate(l.date)}</div></td><td className="desc">{l.description}<div className="muted small">{l.tax_code}</div></td>
              <td className="num">{formatEuro(l.base_cents)}</td><td className="num">{formatEuro(l.vat_cents)}</td></tr>
          ))}</tbody>
        </table>
      )}
    </aside>
  );
}

function PlatformContacts({ onChanged }: { onChanged: () => void }) {
  const contacts = useLoad(() => api.contacts(), []);
  const apple = useLoad(() => api.platformContact("apple"), []);
  const google = useLoad(() => api.platformContact("google"), []);
  const platforms = (contacts.data ?? []).filter((c) => c.is_platform);
  const row = (platform: string, current: string | null | undefined, reload: () => void, label: string) => (
    <label key={platform}>{label}
      <select value={current ?? ""} onChange={(e) => api.setPlatformContact(platform, e.target.value || null).then(() => { reload(); onChanged(); })}>
        <option value="">— kies contact (rol platform) —</option>
        {platforms.map((c) => <option key={c.id} value={c.id}>{c.name} {c.vat_number ? `(${c.vat_number})` : ""}</option>)}
      </select>
    </label>
  );
  return (
    <details className="small">
      <summary>Afnemers van platformomzet voor de ICP-opgaaf</summary>
      <p className="muted">Maak de Ierse entiteiten van Apple en Google aan als contact met rol “Platform” en hun btw-id, en koppel ze hier.</p>
      <div className="form-grid">{row("apple", apple.data, apple.reload, "Apple")}{row("google", google.data, google.reload, "Google")}</div>
    </details>
  );
}
