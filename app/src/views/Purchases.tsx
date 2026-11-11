import { useState } from "react";
import { api, type DocumentInfo, type PurchaseForm, type PurchaseLineForm, type PurchaseSummary } from "../api";
import { DocumentUpload, DocumentViewer } from "../components/DocumentViewer";
import { useLoad } from "../hooks";
import { centsToInput, formatDate, formatEuro, parseCents, todayIso, vatFor } from "../money";
import { ContactForm, emptyContact } from "./Contacts";

const stateLabel: Record<PurchaseSummary["state"], string> = {
  open: "Open", paid: "Betaald", paid_privately: "Privé betaald", reversed: "Tegengeboekt",
};

export function Purchases({ year }: { year: number }) {
  const list = useLoad(() => api.purchaseInvoices(year), [year]);
  const [selected, setSelected] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  if (creating) return <PurchaseEditor onDone={(id) => { setCreating(false); list.reload(); if (id) setSelected(id); }} />;
  const missingDocs = (list.data ?? []).filter((p) => !p.has_document && p.state !== "reversed").length;
  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Inkoopfacturen {year}</h2>
          <button className="primary" onClick={() => setCreating(true)}>+ Inkoopfactuur</button>
        </header>
        {missingDocs > 0 && <p className="warning small">{missingDocs} inkoopfactu{missingDocs === 1 ? "ur" : "ren"} zonder bewijsstuk.</p>}
        {list.error && <p className="error">{list.error}</p>}
        {list.data?.length === 0 && <p className="muted">Nog geen inkoopfacturen in {year}.</p>}
        {list.data && list.data.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Datum</th><th>Leverancier</th><th>Omschrijving</th><th>Status</th><th className="num">Totaal</th><th>Bon</th></tr></thead>
            <tbody>
              {list.data.map((p) => (
                <tr key={p.id} className={(selected === p.id ? "selected " : "") + (p.state === "reversed" ? "inactive" : "")} onClick={() => setSelected(p.id)}>
                  <td>{formatDate(p.invoice_date)}</td>
                  <td>{p.contact_name}</td>
                  <td className="desc">{p.description}</td>
                  <td><span className="tag">{stateLabel[p.state]}</span></td>
                  <td className="num">{formatEuro(p.total_cents)}</td>
                  <td>{p.has_document ? "✓" : <span className="warning">ontbreekt</span>}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {selected && <PurchasePanel id={selected} onChanged={list.reload} />}
    </section>
  );
}

function PurchasePanel({ id, onChanged }: { id: string; onChanged: () => void }) {
  const detail = useLoad(() => api.purchaseInvoice(id), [id]);
  const [error, setError] = useState<string | null>(null);
  const [payDate, setPayDate] = useState(todayIso());
  const run = (p: Promise<unknown>) => { setError(null); p.then(() => { detail.reload(); onChanged(); }).catch((e) => setError(String(e))); };
  const d = detail.data;
  if (detail.error) return <aside className="panel"><p className="error">{detail.error}</p></aside>;
  if (!d) return <aside className="panel" />;
  return (
    <aside className="panel">
      <h3>{d.summary.contact_name}</h3>
      <p>{d.summary.description}<br /><span className="muted small">{formatDate(d.summary.invoice_date)}{d.summary.supplier_reference && ` · factuur ${d.summary.supplier_reference}`}</span></p>
      <table>
        <tbody>
          {d.lines.map((l, i) => <tr key={i}><td>{l.account} {l.description}</td><td>{l.tax_code}</td><td className="num">{formatEuro(l.net_cents)}</td><td className="num">{formatEuro(l.vat_cents)}</td></tr>)}
        </tbody>
        <tfoot><tr><td colSpan={3}>Totaal te betalen</td><td className="num">{formatEuro(d.summary.total_cents)}</td></tr></tfoot>
      </table>
      {error && <p className="error">{error}</p>}
      {d.summary.state === "open" && (
        <div className="box">
          <strong className="small">Betaling registreren ({formatEuro(d.summary.open_cents)} open)</strong>
          <div className="row gap">
            <input type="date" value={payDate} onChange={(e) => setPayDate(e.target.value)} />
            <button onClick={() => run(api.registerPurchasePayment(id, payDate, centsToInput(d.summary.open_cents)))}>Volledig betaald</button>
          </div>
        </div>
      )}
      {d.document_id
        ? <DocumentViewer id={d.document_id} height={420} />
        : <div className="box"><span className="warning small">Geen bewijsstuk gekoppeld.</span>
            <DocumentUpload type="purchase_invoice" label="Bewijsstuk toevoegen…" onUploaded={(doc) => run(api.attachPurchaseDocument(id, doc.id))} />
          </div>}
    </aside>
  );
}

const emptyLine = (code: string): PurchaseLineForm => ({ description: null, account: "", net: "", tax_code: code, vat: "" });

function PurchaseEditor({ onDone }: { onDone: (id: string | null) => void }) {
  const contacts = useLoad(() => api.contacts(), []);
  const accounts = useLoad(() => api.accounts(), []);
  const [form, setForm] = useState<PurchaseForm>({
    contact_id: "", supplier_reference: null, invoice_date: todayIso(), due_date: null, description: "",
    paid_privately: false, document_id: null, lines: [emptyLine("I21")],
  });
  const taxCodes = useLoad(() => api.taxCodes(form.invoice_date || todayIso()), [form.invoice_date]);
  const regime = useLoad(() => api.vatRegime(form.invoice_date || todayIso()), [form.invoice_date]);
  const [doc, setDoc] = useState<DocumentInfo | null>(null);
  const [newContact, setNewContact] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const purchaseCodes = (taxCodes.data ?? []).filter((t) => t.kind !== "sales");
  const setLine = (i: number, patch: Partial<PurchaseLineForm>) =>
    setForm((f) => ({ ...f, lines: f.lines.map((l, j) => (j === i ? { ...l, ...patch } : l)) }));
  const suggestVat = (net: string, code: string) => {
    const cents = parseCents(net);
    const tc = purchaseCodes.find((t) => t.code === code);
    return cents !== null && tc?.rate_bp && tc.kind === "purchase" ? centsToInput(vatFor(cents, tc.rate_bp)) : "";
  };
  const total = form.lines.reduce((s, l) => {
    const tc = purchaseCodes.find((t) => t.code === l.tax_code);
    const vat = tc && !tc.rubric?.startsWith("4") ? parseCents(l.vat) ?? 0 : 0;
    return s + (parseCents(l.net) ?? 0) + vat;
  }, 0);
  const pickContact = (id: string) => {
    const c = contacts.data?.find((x) => x.id === id);
    setForm((f) => ({
      ...f,
      contact_id: id,
      lines: f.lines.map((l, i) => i === 0 && !l.account ? { ...l, account: c?.default_account ?? "", tax_code: c?.default_tax_code ?? l.tax_code } : l),
    }));
  };
  const save = () => api.registerPurchase({ ...form, document_id: doc?.id ?? null }).then((s) => onDone(s.id)).catch((e) => setError(String(e)));

  return (
    <section className="split wide-right">
      <div>
        <header className="row">
          <h2>Inkoopfactuur registreren</h2>
          <button onClick={() => onDone(null)}>Terug</button>
        </header>
        <div className="form-grid">
          <label className="wide">Leverancier
            <span className="row gap">
              <select value={form.contact_id} onChange={(e) => pickContact(e.target.value)} style={{ flex: 1 }}>
                <option value="">— kies leverancier —</option>
                {(contacts.data ?? []).filter((c) => c.is_supplier || c.id === form.contact_id).map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
              <button onClick={() => setNewContact(true)}>Nieuw…</button>
            </span>
          </label>
          <label>Factuurdatum <input type="date" value={form.invoice_date} onChange={(e) => setForm({ ...form, invoice_date: e.target.value })} /></label>
          <label>Factuurnummer leverancier <input value={form.supplier_reference ?? ""} onChange={(e) => setForm({ ...form, supplier_reference: e.target.value || null })} /></label>
          <label className="wide">Omschrijving <input value={form.description} onChange={(e) => setForm({ ...form, description: e.target.value })} /></label>
        </div>
        {newContact && (
          <ContactForm initial={{ ...emptyContact(), is_customer: false, is_supplier: true }} onCancel={() => setNewContact(false)}
            onSaved={(id) => { setNewContact(false); contacts.reload(); pickContact(id); }} />
        )}
        {regime.data === "kor" && <p className="muted small">Onder de KOR is btw niet aftrekbaar: boek het bedrag inclusief btw met code GEEN.</p>}
        <table className="entry-lines">
          <thead><tr><th>Kostenrekening</th><th>Btw-code</th><th className="num">Bedrag excl.</th><th className="num">Btw</th><th /></tr></thead>
          <tbody>
            {form.lines.map((l, i) => (
              <tr key={i}>
                <td>
                  <select value={l.account} onChange={(e) => setLine(i, { account: e.target.value })}>
                    <option value="">— kies —</option>
                    {(accounts.data ?? []).filter((a) => a.active && !["revenue"].includes(a.account_type)).map((a) => <option key={a.number} value={a.number}>{a.number} {a.name}</option>)}
                  </select>
                </td>
                <td>
                  <select value={l.tax_code} onChange={(e) => setLine(i, { tax_code: e.target.value, vat: suggestVat(l.net, e.target.value) })}>
                    {purchaseCodes.map((t) => <option key={t.code} value={t.code}>{t.code} {t.name}</option>)}
                  </select>
                </td>
                <td><input className="num" value={l.net} onChange={(e) => setLine(i, { net: e.target.value, vat: suggestVat(e.target.value, l.tax_code) })} /></td>
                <td><input className="num" value={l.vat} onChange={(e) => setLine(i, { vat: e.target.value })} title="Neem het btw-bedrag van de factuur over" /></td>
                <td>{form.lines.length > 1 && <button onClick={() => setForm({ ...form, lines: form.lines.filter((_, j) => j !== i) })}>×</button>}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr><td colSpan={2}><button onClick={() => setForm({ ...form, lines: [...form.lines, emptyLine(form.lines[0]?.tax_code ?? "I21")] })}>+ Regel</button></td>
              <td className="num" colSpan={2}><strong>Te betalen {formatEuro(total)}</strong></td><td /></tr>
          </tfoot>
        </table>
        <label className="check"><input type="checkbox" checked={form.paid_privately} onChange={(e) => setForm({ ...form, paid_privately: e.target.checked })} /> Betaald vanaf privérekening (boekt op Privé-stortingen)</label>
        {error && <p className="error">{error}</p>}
        <div className="row gap" style={{ marginTop: 12 }}>
          <button className="primary" onClick={save}>Registreren en boeken</button>
          {!doc && <span className="warning small">Nog geen bewijsstuk toegevoegd.</span>}
        </div>
      </div>
      <aside className="panel">
        <h3>Bewijsstuk</h3>
        {doc ? <DocumentViewer id={doc.id} height={560} /> : <p className="muted small">Voeg de factuur of bon toe (PDF of foto); je ziet hem hier naast de gegevens.</p>}
        <DocumentUpload type="purchase_invoice" label={doc ? "Ander bestand…" : "Bestand kiezen…"} onUploaded={setDoc} />
      </aside>
    </section>
  );
}
