import { useState } from "react";
import { api, entryNumber, journals, type NewLine } from "../api";
import { useLoad } from "../hooks";
import { formatEuro, parseCents, todayIso } from "../money";

const emptyLine = (): NewLine => ({ account: "", debit: "", credit: "", tax_code: null, description: null });

export function NewEntry({ onPosted }: { onPosted: () => void }) {
  const [journal, setJournal] = useState("MEM");
  const [date, setDate] = useState(todayIso());
  const [description, setDescription] = useState("");
  const [lines, setLines] = useState<NewLine[]>([emptyLine(), emptyLine()]);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const accounts = useLoad(() => api.accounts(), []);
  const taxCodes = useLoad(() => (date ? api.taxCodes(date) : Promise.resolve([])), [date]);

  const debit = lines.reduce((s, l) => s + (parseCents(l.debit) ?? 0), 0);
  const credit = lines.reduce((s, l) => s + (parseCents(l.credit) ?? 0), 0);
  const diff = debit - credit;

  const update = (i: number, patch: Partial<NewLine>) =>
    setLines((ls) => ls.map((l, j) => (j === i ? { ...l, ...patch } : l)));

  const fillDifference = (i: number) => {
    if (diff > 0) update(i, { credit: formatEuro(diff).replace("€ ", ""), debit: "" });
    if (diff < 0) update(i, { debit: formatEuro(-diff).replace("€ ", ""), credit: "" });
  };

  const submit = () => {
    setError(null);
    setSuccess(null);
    api
      .postEntry({ journal, date, description, lines })
      .then((p) => {
        setSuccess(`Geboekt als ${entryNumber(p)}.`);
        setDescription("");
        setLines([emptyLine(), emptyLine()]);
        onPosted();
      })
      .catch((e) => setError(String(e)));
  };

  return (
    <section>
      <h2>Nieuwe boeking</h2>
      <div className="form-grid">
        <label>Dagboek
          <select value={journal} onChange={(e) => setJournal(e.target.value)}>
            {journals.map((j) => <option key={j.code} value={j.code}>{j.name}</option>)}
          </select>
        </label>
        <label>Datum <input type="date" value={date} onChange={(e) => setDate(e.target.value)} /></label>
        <label className="wide">Omschrijving <input value={description} onChange={(e) => setDescription(e.target.value)} /></label>
      </div>

      <table className="entry-lines">
        <thead><tr><th>Rekening</th><th>Btw-code</th><th>Regelomschrijving</th><th className="num">Debet</th><th className="num">Credit</th><th /></tr></thead>
        <tbody>
          {lines.map((l, i) => (
            <tr key={i}>
              <td>
                <select value={l.account} onChange={(e) => update(i, { account: e.target.value })}>
                  <option value="">— kies —</option>
                  {accounts.data?.filter((a) => a.active).map((a) => (
                    <option key={a.number} value={a.number}>{a.number} {a.name}</option>
                  ))}
                </select>
              </td>
              <td>
                <select value={l.tax_code ?? ""} onChange={(e) => update(i, { tax_code: e.target.value || null })}>
                  <option value="">—</option>
                  {taxCodes.data?.map((t) => <option key={t.code} value={t.code} title={t.note ?? ""}>{t.code} {t.name}</option>)}
                </select>
              </td>
              <td><input value={l.description ?? ""} onChange={(e) => update(i, { description: e.target.value || null })} /></td>
              <td><input className={"num" + (l.debit && parseCents(l.debit) === null ? " invalid" : "")} value={l.debit}
                         onChange={(e) => update(i, { debit: e.target.value, credit: e.target.value ? "" : l.credit })} /></td>
              <td><input className={"num" + (l.credit && parseCents(l.credit) === null ? " invalid" : "")} value={l.credit}
                         onChange={(e) => update(i, { credit: e.target.value, debit: e.target.value ? "" : l.debit })} /></td>
              <td className="actions">
                {diff !== 0 && !l.debit && !l.credit && <button title="Verschil invullen" onClick={() => fillDifference(i)}>=</button>}
                {lines.length > 2 && <button title="Regel verwijderen" onClick={() => setLines((ls) => ls.filter((_, j) => j !== i))}>×</button>}
              </td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td colSpan={3}><button onClick={() => setLines((ls) => [...ls, emptyLine()])}>+ Regel</button></td>
            <td className="num">{formatEuro(debit)}</td>
            <td className="num">{formatEuro(credit)}</td>
            <td />
          </tr>
          <tr>
            <td colSpan={3} />
            <td colSpan={2} className={"num " + (diff === 0 ? "ok" : "error")}>
              {debit === 0 && credit === 0 ? "" : diff === 0 ? "In balans" : `Verschil ${formatEuro(Math.abs(diff))}`}
            </td>
            <td />
          </tr>
        </tfoot>
      </table>

      {error && <p className="error">{error}</p>}
      {success && <p className="ok">{success}</p>}
      <button className="primary" disabled={diff !== 0 || debit === 0} onClick={submit}>Definitief boeken</button>
      <p className="muted small">Een definitieve boeking kan niet meer worden gewijzigd, alleen tegengeboekt.</p>
    </section>
  );
}
