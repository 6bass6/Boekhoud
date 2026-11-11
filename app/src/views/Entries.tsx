import { useState } from "react";
import { api, journals, type EntryDetail } from "../api";
import { useLoad } from "../hooks";
import { formatDate, formatEuro, todayIso } from "../money";

export function Entries({ year }: { year: number }) {
  const [journal, setJournal] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const list = useLoad(() => api.listEntries(year, journal || null), [year, journal]);

  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Boekingen {year}</h2>
          <select value={journal} onChange={(e) => setJournal(e.target.value)}>
            <option value="">Alle dagboeken</option>
            {journals.map((j) => <option key={j.code} value={j.code}>{j.name}</option>)}
          </select>
        </header>
        {list.error && <p className="error">{list.error}</p>}
        {list.data?.length === 0 && <p className="muted">Nog geen boekingen in {year}.</p>}
        {list.data && list.data.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Nummer</th><th>Datum</th><th>Omschrijving</th><th className="num">Bedrag</th></tr></thead>
            <tbody>
              {list.data.map((e) => (
                <tr key={e.id} className={selected === e.id ? "selected" : ""} onClick={() => setSelected(e.id)}>
                  <td>{e.number}</td>
                  <td>{formatDate(e.date)}</td>
                  <td className="desc">
                    {e.description}
                    {e.reversed_by_entry_id && <span className="tag">tegengeboekt</span>}
                    {e.reverses_entry_id && <span className="tag">correctie</span>}
                  </td>
                  <td className="num">{formatEuro(e.total_debit_cents)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {selected && <EntryPanel id={selected} onChanged={list.reload} onSelect={setSelected} />}
    </section>
  );
}

function EntryPanel({ id, onChanged, onSelect }: { id: string; onChanged: () => void; onSelect: (id: string) => void }) {
  const detail = useLoad(() => api.entryDetail(id), [id]);
  const [reversing, setReversing] = useState(false);

  if (detail.error) return <aside className="panel"><p className="error">{detail.error}</p></aside>;
  if (!detail.data) return <aside className="panel" />;
  const d = detail.data;
  return (
    <aside className="panel">
      <h3>{d.summary.number}</h3>
      <p>{d.summary.description}<br /><span className="muted">{formatDate(d.summary.date)} · bron: {d.summary.source_kind}</span></p>
      <Lines detail={d} />
      {d.summary.reverses_entry_id && (
        <p>Corrigeert <a onClick={() => onSelect(d.summary.reverses_entry_id!)}>de oorspronkelijke boeking</a>.</p>
      )}
      {d.summary.reversed_by_entry_id ? (
        <p>Tegengeboekt door <a onClick={() => onSelect(d.summary.reversed_by_entry_id!)}>correctieboeking</a>.</p>
      ) : !d.summary.reverses_entry_id && (
        reversing
          ? <ReverseForm id={id} onDone={(newId) => { setReversing(false); onChanged(); onSelect(newId); }} onCancel={() => setReversing(false)} />
          : <button onClick={() => setReversing(true)}>Tegenboeken…</button>
      )}
      <p className="muted small">Geboekt {d.posted_at.slice(0, 19).replace("T", " ")} UTC<br />Hash {d.hash.slice(0, 16)}…</p>
    </aside>
  );
}

function Lines({ detail }: { detail: EntryDetail }) {
  return (
    <table>
      <thead><tr><th>Rekening</th><th>Btw</th><th className="num">Debet</th><th className="num">Credit</th></tr></thead>
      <tbody>
        {detail.lines.map((l) => (
          <tr key={l.line_no}>
            <td className="desc">{l.account} {l.account_name}{l.description && <div className="muted small">{l.description}</div>}</td>
            <td>{l.tax_code ?? ""}</td>
            <td className="num">{l.amount_cents > 0 ? formatEuro(l.amount_cents) : ""}</td>
            <td className="num">{l.amount_cents < 0 ? formatEuro(-l.amount_cents) : ""}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ReverseForm({ id, onDone, onCancel }: { id: string; onDone: (id: string) => void; onCancel: () => void }) {
  const [date, setDate] = useState(todayIso());
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = () =>
    api.reverseEntry(id, date, reason).then((p) => onDone(p.id)).catch((e) => setError(String(e)));
  return (
    <div className="box">
      <p className="small">Een definitieve boeking wordt nooit gewijzigd. Er wordt een tegenboeking gemaakt met omgekeerde bedragen; daarna kun je de juiste boeking opnieuw invoeren.</p>
      <label>Datum tegenboeking <input type="date" value={date} onChange={(e) => setDate(e.target.value)} /></label>
      <label>Reden <input value={reason} onChange={(e) => setReason(e.target.value)} placeholder="bijv. verkeerde rekening" /></label>
      {error && <p className="error">{error}</p>}
      <div className="row gap">
        <button className="primary" onClick={submit}>Tegenboeking maken</button>
        <button onClick={onCancel}>Annuleren</button>
      </div>
    </div>
  );
}

