import { useState } from "react";
import { api } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro, todayIso } from "../money";

export function Assets() {
  const list = useLoad(() => api.assets(), []);
  const [form, setForm] = useState({ name: "", acquisition_date: todayIso(), cost: "", residual: "0", years: "5", business_use_pct: 100, notes: "" });
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const report = (p: Promise<unknown>, ok: (v: unknown) => string) => p.then((v) => { setMessage({ ok: true, text: ok(v) }); list.reload(); }).catch((e) => setMessage({ ok: false, text: String(e) }));

  return (
    <section>
      <header className="row">
        <h2>Activa en afschrijvingen</h2>
        <button onClick={() => report(api.postDepreciation(todayIso()), (n) => `${n} afschrijvingsboeking(en) gemaakt t/m deze maand.`)}>Afschrijvingen boeken t/m deze maand</button>
      </header>
      <p className="muted small">Registreer hier bedrijfsmiddelen die je meerdere jaren gebruikt (bijv. laptop, telefoon). Boek de aanschaf zelf via Inkoopfacturen op rekening 0100. Afschrijving: lineair per maand, de laatste maand vangt afronding op.</p>
      {message && <p className={message.ok ? "ok" : "error"}>{message.text}</p>}
      {list.data && list.data.length > 0 && (
        <table>
          <thead><tr><th>Naam</th><th>Aanschaf</th><th className="num">Prijs</th><th className="num">Per maand</th><th className="num">Afgeschreven</th><th className="num">Boekwaarde</th><th>Voortgang</th></tr></thead>
          <tbody>{list.data.map((a) => (
            <tr key={a.id}><td className="desc">{a.name}{a.business_use_pct < 100 && <div className="muted small">{a.business_use_pct}% zakelijk</div>}</td>
              <td>{formatDate(a.acquisition_date)}</td><td className="num">{formatEuro(a.cost_cents)}</td><td className="num">{formatEuro(a.monthly_cents)}</td>
              <td className="num">{formatEuro(a.depreciated_cents)}</td><td className="num">{formatEuro(a.book_value_cents)}</td><td>{a.months_posted}/{a.months} mnd</td></tr>
          ))}</tbody>
        </table>
      )}
      <div className="panel" style={{ marginTop: 16 }}>
        <h3>Bedrijfsmiddel toevoegen</h3>
        <div className="form-grid">
          <label className="wide">Naam <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} /></label>
          <label>Aanschafdatum <input type="date" value={form.acquisition_date} onChange={(e) => setForm({ ...form, acquisition_date: e.target.value })} /></label>
          <label>Aanschafprijs excl. btw <input className="num" value={form.cost} onChange={(e) => setForm({ ...form, cost: e.target.value })} /></label>
          <label>Restwaarde <input className="num" value={form.residual} onChange={(e) => setForm({ ...form, residual: e.target.value })} /></label>
          <label>Looptijd (jaren) <input value={form.years} onChange={(e) => setForm({ ...form, years: e.target.value })} /></label>
          <label>Zakelijk gebruik (%) <input type="number" min={0} max={100} value={form.business_use_pct} onChange={(e) => setForm({ ...form, business_use_pct: Number(e.target.value) })} /></label>
        </div>
        <button className="primary" onClick={() => report(api.addAsset({ ...form, notes: form.notes || null }), () => "Bedrijfsmiddel toegevoegd.")}>Toevoegen</button>
      </div>
    </section>
  );
}
