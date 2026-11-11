import { useEffect, useMemo, useState } from "react";
import { api, type SalesInvoiceDetail, type SalesInvoiceForm, type SalesLineForm, type SalesInvoiceSummary, type TaxCode } from "../api";
import { useLoad } from "../hooks";
import { addDays, centsToInput, formatDate, formatEuro, formatQuantity, lineNet, parseCents, parseQuantity, todayIso, vatFor } from "../money";

export const invoiceStateLabel: Record<SalesInvoiceSummary["state"], string> = {
  draft: "Concept", open: "Open", partially_paid: "Deels betaald", paid: "Betaald",
  overdue: "Vervallen", credited: "Gecrediteerd", credit_note: "Creditnota",
};

export function SalesInvoices({ year }: { year: number }) {
  const list = useLoad(() => api.salesInvoices(year), [year]);
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | "new" | null>(null);

  const open = (list.data ?? []).filter((i) => i.status === "issued").reduce((s, i) => s + i.open_cents, 0);
  const done = () => { setEditing(null); list.reload(); };

  if (editing) {
    return <SalesInvoiceEditor id={editing === "new" ? null : editing} onDone={(id) => { done(); if (id) setSelected(id); }} />;
  }
  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Verkoopfacturen {year}</h2>
          <button className="primary" onClick={() => setEditing("new")}>+ Factuur</button>
        </header>
        <p className="muted small">Openstaand: {formatEuro(open)}</p>
        {list.error && <p className="error">{list.error}</p>}
        {list.data?.length === 0 && <p className="muted">Nog geen facturen in {year}.</p>}
        {list.data && list.data.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Nummer</th><th>Datum</th><th>Klant</th><th>Status</th><th className="num">Totaal</th><th className="num">Open</th></tr></thead>
            <tbody>
              {list.data.map((i) => (
                <tr key={i.id} className={selected === i.id ? "selected" : ""} onClick={() => setSelected(i.id)}>
                  <td>{i.invoice_number ?? "—"}</td>
                  <td>{formatDate(i.invoice_date)}</td>
                  <td className="desc">{i.contact_name}</td>
                  <td><span className={"tag state-" + i.state}>{invoiceStateLabel[i.state]}</span></td>
                  <td className="num">{formatEuro(i.total_cents)}</td>
                  <td className="num">{i.status === "issued" && !i.is_credit_note ? formatEuro(i.open_cents) : ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {selected && <SalesInvoicePanel id={selected} onEdit={() => setEditing(selected)} onChanged={list.reload} onSelect={setSelected} />}
    </section>
  );
}

function SalesInvoicePanel({ id, onEdit, onChanged, onSelect }: { id: string; onEdit: () => void; onChanged: () => void; onSelect: (id: string) => void }) {
  const detail = useLoad(() => api.salesInvoice(id), [id]);
  const req = useLoad(() => api.salesIssueRequirements(id), [id]);
  const [error, setError] = useState<string | null>(null);
  const [payDate, setPayDate] = useState(todayIso());
  const [payAmount, setPayAmount] = useState("");

  useEffect(() => { if (detail.data) setPayAmount(centsToInput(detail.data.summary.open_cents)); }, [detail.data]);

  const run = (p: Promise<unknown>, after?: () => void) => {
    setError(null);
    p.then(() => { detail.reload(); req.reload(); onChanged(); after?.(); }).catch((e) => setError(String(e)));
  };
  if (detail.error) return <aside className="panel"><p className="error">{detail.error}</p></aside>;
  const d = detail.data;
  if (!d) return <aside className="panel" />;
  const s = d.summary;
  const draft = s.status === "draft";

  return (
    <aside className="panel">
      <h3>{s.is_credit_note ? "Creditnota" : "Factuur"} {s.invoice_number ?? "(concept)"}</h3>
      <p>{s.contact_name}<br /><span className="muted small">{formatDate(s.invoice_date)}{!s.is_credit_note && ` · vervalt ${formatDate(s.due_date)}`}</span></p>
      {d.credits_invoice_id && <p className="small">Crediteert <a onClick={() => onSelect(d.credits_invoice_id!)}>factuur {d.credits_invoice_number}</a></p>}
      <InvoiceLinesTable detail={d} />
      {error && <p className="error">{error}</p>}
      <div className="row gap wrap">
        <button onClick={() => api.openSalesInvoicePdf(id).catch((e) => setError(String(e)))}>{draft ? "Voorbeeld PDF" : "PDF openen"}</button>
        {draft && <button onClick={onEdit}>Bewerken</button>}
        {draft && <button onClick={() => run(api.deleteSalesInvoice(id), () => onSelect(""))}>Verwijderen</button>}
      </div>
      {draft && req.data && (
        req.data.missing.length > 0
          ? <div className="box"><strong className="small">Nog nodig voor uitreiken:</strong><ul className="small">{req.data.missing.map((m) => <li key={m}>{m}</li>)}</ul></div>
          : <div className="box">
              <p className="small">Bij uitreiken krijgt de factuur het volgende nummer, wordt hij geboekt in het verkoopboek en kan hij niet meer worden gewijzigd.</p>
              <button className="primary" onClick={() => run(api.issueSalesInvoice(id))}>Factuur uitreiken</button>
            </div>
      )}
      {!draft && !s.is_credit_note && s.open_cents !== 0 && (
        <div className="box">
          <strong className="small">Betaling registreren</strong>
          <p className="muted small">Ontvangsten via de bankimport worden automatisch gekoppeld; gebruik dit voor andere betalingen.</p>
          <div className="row gap">
            <input type="date" value={payDate} onChange={(e) => setPayDate(e.target.value)} />
            <input className="num" value={payAmount} onChange={(e) => setPayAmount(e.target.value)} />
            <button onClick={() => run(api.registerSalesPayment(id, payDate, payAmount))}>Registreren</button>
          </div>
        </div>
      )}
      {!draft && !s.is_credit_note && s.state !== "credited" && (
        <button onClick={() => api.createCreditNote(id, todayIso()).then((cn) => { onChanged(); onSelect(cn); }).catch((e) => setError(String(e)))}>
          Creditnota maken…
        </button>
      )}
      {d.payments.length > 0 && (
        <>
          <h4>Betalingen</h4>
          <table><tbody>{d.payments.map((p) => (
            <tr key={p.id} className={p.reversed ? "inactive" : ""}><td>{formatDate(p.date)}</td><td>{p.method}{p.reversed && " (tegengeboekt)"}</td><td className="num">{formatEuro(p.amount_cents)}</td></tr>
          ))}</tbody></table>
        </>
      )}
    </aside>
  );
}

function InvoiceLinesTable({ detail }: { detail: SalesInvoiceDetail }) {
  return (
    <table>
      <thead><tr><th>Omschrijving</th><th className="num">Aantal</th><th className="num">Bedrag</th></tr></thead>
      <tbody>
        {detail.lines.map((l, i) => (
          <tr key={i}>
            <td className="desc">{l.description}<div className="muted small">{l.tax_code} · {formatEuro(l.unit_price_cents)}</div></td>
            <td className="num">{formatQuantity(l.quantity_milli)}</td>
            <td className="num">{formatEuro(lineNet(l.quantity_milli, l.unit_price_cents))}</td>
          </tr>
        ))}
      </tbody>
      {detail.totals && (
        <tfoot>
          <tr><td colSpan={2}>Subtotaal</td><td className="num">{formatEuro(detail.totals.subtotal_cents)}</td></tr>
          {detail.totals.groups.filter((g) => g.vat_cents !== 0).map((g) => (
            <tr key={g.code}><td colSpan={2}>Btw {g.code}</td><td className="num">{formatEuro(g.vat_cents)}</td></tr>
          ))}
          <tr><td colSpan={2}>Totaal</td><td className="num">{formatEuro(detail.totals.total_cents)}</td></tr>
        </tfoot>
      )}
    </table>
  );
}

const emptyLine = (taxCode: string): SalesLineForm => ({ description: "", quantity: "1", unit_price: "", tax_code: taxCode, account: "8100" });

function SalesInvoiceEditor({ id, onDone }: { id: string | null; onDone: (id: string | null) => void }) {
  const contacts = useLoad(() => api.contacts(), []);
  const accounts = useLoad(() => api.accounts(), []);
  const company = useLoad(() => api.company(), []);
  const [form, setForm] = useState<SalesInvoiceForm>({
    contact_id: "", invoice_date: todayIso(), supply_date: null, due_date: null, reference: null, notes: null, lines: [],
  });
  const regime = useLoad(() => api.vatRegime(form.invoice_date || todayIso()), [form.invoice_date]);
  const taxCodes = useLoad(() => api.taxCodes(form.invoice_date || todayIso()), [form.invoice_date]);
  const [error, setError] = useState<string | null>(null);
  const defaultCode = regime.data === "kor" ? "VKOR" : "V21";

  useEffect(() => {
    if (id) {
      api.salesInvoice(id).then((d) => setForm({
        contact_id: d.summary.contact_id,
        invoice_date: d.summary.invoice_date,
        supply_date: d.supply_date,
        due_date: d.summary.due_date,
        reference: d.reference,
        notes: d.notes,
        lines: d.lines.map((l) => ({ description: l.description, quantity: formatQuantity(l.quantity_milli), unit_price: centsToInput(l.unit_price_cents), tax_code: l.tax_code, account: l.account })),
      })).catch((e) => setError(String(e)));
    }
  }, [id]);
  useEffect(() => {
    if (!id && form.lines.length === 0 && regime.data) setForm((f) => ({ ...f, lines: [emptyLine(defaultCode)] }));
  }, [id, regime.data, defaultCode, form.lines.length]);

  const salesCodes = (taxCodes.data ?? []).filter((t: TaxCode) => t.kind === "sales");
  const preview = useMemo(() => {
    const groups = new Map<string, number>();
    let ok = true;
    for (const l of form.lines) {
      const q = parseQuantity(l.quantity);
      const p = parseCents(l.unit_price.replace(/^-/, ""));
      if (q === null || p === null) { if (l.description || l.unit_price) ok = false; continue; }
      const sign = l.unit_price.trim().startsWith("-") ? -1 : 1;
      const net = lineNet(q, p * sign);
      groups.set(l.tax_code, (groups.get(l.tax_code) ?? 0) + net);
    }
    let sub = 0, vat = 0;
    for (const [code, base] of groups) {
      const tc = salesCodes.find((t) => t.code === code);
      sub += base;
      if (tc?.rate_bp && tc.rubric?.startsWith("1")) vat += vatFor(base, tc.rate_bp);
    }
    return { ok, sub, vat, total: sub + vat };
  }, [form.lines, salesCodes]);

  const setLine = (i: number, patch: Partial<SalesLineForm>) =>
    setForm((f) => ({ ...f, lines: f.lines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }));
  const save = () => api.saveSalesInvoice(id, form).then((newId) => onDone(newId)).catch((e) => setError(String(e)));
  const customers = (contacts.data ?? []).filter((c) => c.is_customer || c.id === form.contact_id);
  const term = company.data?.payment_term_days ?? 30;

  return (
    <section>
      <header className="row">
        <h2>{id ? "Concept bewerken" : "Nieuwe factuur"}</h2>
        <button onClick={() => onDone(null)}>Terug</button>
      </header>
      <div className="form-grid">
        <label className="wide">Klant
          <select value={form.contact_id} onChange={(e) => setForm({ ...form, contact_id: e.target.value })}>
            <option value="">— kies klant —</option>
            {customers.map((c) => <option key={c.id} value={c.id}>{c.name}{c.city ? ` (${c.city})` : ""}</option>)}
          </select>
        </label>
        <label>Factuurdatum <input type="date" value={form.invoice_date} onChange={(e) => setForm({ ...form, invoice_date: e.target.value })} /></label>
        <label>Leverdatum (als anders) <input type="date" value={form.supply_date ?? ""} onChange={(e) => setForm({ ...form, supply_date: e.target.value || null })} /></label>
        <label>Vervaldatum <input type="date" value={form.due_date ?? addDays(form.invoice_date || todayIso(), term)} onChange={(e) => setForm({ ...form, due_date: e.target.value || null })} /></label>
        <label>Referentie klant <input value={form.reference ?? ""} onChange={(e) => setForm({ ...form, reference: e.target.value || null })} /></label>
      </div>
      {regime.data === "kor" && <p className="muted small">Btw-regime op factuurdatum: KOR — er wordt geen btw berekend.</p>}
      <table className="entry-lines">
        <thead><tr><th>Omschrijving</th><th className="num">Aantal</th><th className="num">Prijs excl.</th><th>Btw</th><th>Omzetrekening</th><th /></tr></thead>
        <tbody>
          {form.lines.map((l, i) => (
            <tr key={i}>
              <td style={{ width: "40%" }}><input value={l.description} placeholder="bijv. Lezing 'Apps bouwen', 12 maart" onChange={(e) => setLine(i, { description: e.target.value })} /></td>
              <td><input className={"num short" + (parseQuantity(l.quantity) === null ? " invalid" : "")} value={l.quantity} onChange={(e) => setLine(i, { quantity: e.target.value })} /></td>
              <td><input className="num" value={l.unit_price} onChange={(e) => setLine(i, { unit_price: e.target.value })} /></td>
              <td>
                <select value={l.tax_code} onChange={(e) => setLine(i, { tax_code: e.target.value })}>
                  {salesCodes.map((t) => <option key={t.code} value={t.code} title={t.note ?? ""}>{t.code} {t.name}</option>)}
                </select>
              </td>
              <td>
                <select value={l.account} onChange={(e) => setLine(i, { account: e.target.value })}>
                  {(accounts.data ?? []).filter((a) => a.account_type === "revenue").map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
                </select>
              </td>
              <td>{form.lines.length > 1 && <button onClick={() => setForm({ ...form, lines: form.lines.filter((_, j) => j !== i) })}>×</button>}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr><td colSpan={6}><button onClick={() => setForm({ ...form, lines: [...form.lines, emptyLine(defaultCode)] })}>+ Regel</button></td></tr>
          <tr><td colSpan={2} /><td className="num">Subtotaal</td><td className="num">{formatEuro(preview.sub)}</td><td colSpan={2} /></tr>
          <tr><td colSpan={2} /><td className="num">Btw</td><td className="num">{formatEuro(preview.vat)}</td><td colSpan={2} /></tr>
          <tr><td colSpan={2} /><td className="num"><strong>Totaal</strong></td><td className="num"><strong>{formatEuro(preview.total)}</strong></td><td colSpan={2} /></tr>
        </tfoot>
      </table>
      <label className="block">Opmerking op factuur
        <textarea rows={2} value={form.notes ?? ""} onChange={(e) => setForm({ ...form, notes: e.target.value || null })} />
      </label>
      {error && <p className="error">{error}</p>}
      <div className="row gap">
        <button className="primary" onClick={save}>Concept opslaan</button>
        <span className="muted small">Uitreiken (nummer toekennen en boeken) doe je daarna vanuit het overzicht.</span>
      </div>
    </section>
  );
}
