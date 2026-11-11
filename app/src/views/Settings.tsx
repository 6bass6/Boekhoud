import { useEffect, useState } from "react";
import { api, type Company, type VatRegime } from "../api";
import { useLoad } from "../hooks";
import { monthNames, todayIso } from "../money";

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

      <h3>Back-up</h3>
      <button onClick={() => api.backupNow().then((p) => setMessage({ ok: true, text: `Back-up gemaakt: ${p}` })).catch((e) => setMessage({ ok: false, text: String(e) }))}>
        Nu back-up maken
      </button>
    </section>
  );
}
