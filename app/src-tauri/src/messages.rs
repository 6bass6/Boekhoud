//! Dutch user-facing messages for errors coming from the domain and storage layers.

use boekhoud_core::ValidationError;
use boekhoud_db::DbError;

pub fn validation_nl(e: &ValidationError) -> String {
    match e {
        ValidationError::TooFewLines => "Een boeking heeft minimaal twee regels nodig.".into(),
        ValidationError::ZeroLine(n) => format!("Regel {n} heeft geen bedrag."),
        ValidationError::NotEur(n, c) => {
            format!("Regel {n} is in {c}; de administratie wordt in euro gevoerd.")
        }
        ValidationError::MissingAccount(n) => format!("Regel {n} heeft geen grootboekrekening."),
        ValidationError::Unbalanced(diff) => {
            format!(
                "De boeking is niet in balans (verschil {}).",
                crate::format_cents(*diff)
            )
        }
        ValidationError::MissingDescription => "Omschrijving ontbreekt.".into(),
        ValidationError::Overflow => "Bedrag is te groot.".into(),
        ValidationError::UnknownAccount(a) => format!("Grootboekrekening {a} bestaat niet."),
        ValidationError::InactiveAccount(a) => format!("Grootboekrekening {a} is niet actief."),
        ValidationError::UnknownTaxCode(c) => {
            format!("Btw-code {c} bestaat niet of is niet geldig op de boekingsdatum.")
        }
        ValidationError::TaxCodeNotAllowed { code, regime } => {
            let r = if regime == "kor" {
                "de KOR"
            } else {
                "het reguliere btw-regime"
            };
            format!("Btw-code {code} mag niet worden gebruikt onder {r}.")
        }
    }
}

pub fn db_nl(e: &DbError) -> String {
    match e {
        DbError::Validation(v) => v.iter().map(validation_nl).collect::<Vec<_>>().join(" "),
        DbError::Duplicate(_) => "Deze gebeurtenis is al eerder geboekt.".into(),
        DbError::PeriodClosed(y, m) => format!("De periode {m:02}-{y} is afgesloten."),
        DbError::NotFound(what) => format!("Niet gevonden: {what}."),
        DbError::AlreadyReversed(_) => "Deze boeking is al tegengeboekt.".into(),
        DbError::Corrupt(what) => format!("Onverwachte gegevens in de database: {what}."),
        DbError::Sqlite(err) => format!("Databasefout: {err}."),
        DbError::Invalid(msg) => {
            let mut c = msg.chars();
            match c.next() {
                Some(f) => format!("{}{}.", f.to_uppercase(), c.as_str()),
                None => String::new(),
            }
        }
        DbError::Io(msg) => format!("Bestandsfout: {msg}."),
    }
}
