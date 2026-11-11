import { useState } from "react";
import { api, type Contact } from "../api";
import { useLoad } from "../hooks";

export const emptyContact = (): Contact => ({
  id: "", name: "", contact_person: null, email: null, address: null, postal_code: null, city: null,
  country: "NL", vat_number: null, kvk_number: null, is_customer: true, is_supplier: false, is_platform: false,
  default_account: null, default_tax_code: null, archived: false,
});

export function Contacts() {
  const [showArchived, setShowArchived] = useState(false);
  const list = useLoad(() => api.contacts(showArchived), [showArchived]);
  const [editing, setEditing] = useState<Contact | null>(null);

  const roles = (c: Contact) => [c.is_customer && "klant", c.is_supplier && "leverancier", c.is_platform && "platform"].filter(Boolean).join(", ");

  return (
    <section className="split">
      <div>
        <header className="row">
          <h2>Contacten</h2>
          <span className="row gap">
            <label className="small"><input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} /> gearchiveerd tonen</label>
            <button className="primary" onClick={() => setEditing(emptyContact())}>+ Contact</button>
          </span>
        </header>
        {list.error && <p className="error">{list.error}</p>}
        {list.data?.length === 0 && <p className="muted">Nog geen contacten.</p>}
        {list.data && list.data.length > 0 && (
          <table className="clickable">
            <thead><tr><th>Naam</th><th>Plaats</th><th>Rol</th><th>Btw-id</th></tr></thead>
            <tbody>
              {list.data.map((c) => (
                <tr key={c.id} className={(editing?.id === c.id ? "selected " : "") + (c.archived ? "inactive" : "")} onClick={() => setEditing(c)}>
                  <td className="desc">{c.name}</td><td>{c.city}</td><td>{roles(c)}</td><td>{c.vat_number}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {editing && <ContactForm key={editing.id || "new"} initial={editing} onSaved={() => { setEditing(null); list.reload(); }} onCancel={() => setEditing(null)} />}
    </section>
  );
}

export function ContactForm({ initial, onSaved, onCancel }: { initial: Contact; onSaved: (id: string) => void; onCancel: () => void }) {
  const [c, setC] = useState<Contact>(initial);
  const [error, setError] = useState<string | null>(null);
  const field = (key: keyof Contact, label: string, wide = false) => (
    <label className={wide ? "wide" : ""}>{label}
      <input value={(c[key] as string | null) ?? ""} onChange={(e) => setC({ ...c, [key]: e.target.value })} />
    </label>
  );
  const flag = (key: "is_customer" | "is_supplier" | "is_platform" | "archived", label: string) => (
    <label className="check"><input type="checkbox" checked={c[key]} onChange={(e) => setC({ ...c, [key]: e.target.checked })} /> {label}</label>
  );
  return (
    <aside className="panel">
      <h3>{c.id ? c.name : "Nieuw contact"}</h3>
      <div className="form-grid two">
        {field("name", "Naam", true)}
        {field("contact_person", "T.a.v.")}
        {field("email", "E-mail")}
        <label className="wide">Adres
          <textarea rows={2} value={c.address ?? ""} onChange={(e) => setC({ ...c, address: e.target.value })} />
        </label>
        {field("postal_code", "Postcode")}
        {field("city", "Plaats")}
        {field("country", "Land (ISO, bijv. NL, IE)")}
        {field("vat_number", "Btw-id")}
        {field("kvk_number", "KvK-nummer")}
      </div>
      <div className="row gap wrap">
        {flag("is_customer", "Klant")}{flag("is_supplier", "Leverancier")}{flag("is_platform", "Platform")}{flag("archived", "Gearchiveerd")}
      </div>
      {error && <p className="error">{error}</p>}
      <div className="row gap" style={{ marginTop: 12 }}>
        <button className="primary" onClick={() => api.saveContact(c).then(onSaved).catch((e) => setError(String(e)))}>Opslaan</button>
        <button onClick={onCancel}>Annuleren</button>
      </div>
    </aside>
  );
}
