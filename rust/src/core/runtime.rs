//! Eine eigene, kleine Tokio-Laufzeit für alle Netzwerkzugriffe.
//!
//! WARUM eine eigene? `reqwest` braucht eine Tokio-Laufzeit. Wir machen uns
//! damit unabhängig davon, wie flutter_rust_bridge intern Aufgaben ausführt.
//! Die API-Funktionen sind normale (nicht-`sync`) Funktionen: Die Brücke
//! ruft sie in einem eigenen Thread-Pool auf, auf Dart-Seite kommt ein
//! `Future` zurück. Dort dürfen wir mit `block_on` warten, ohne die UI zu
//! blockieren.
//!
//! AKKU: Nur 2 Worker-Threads. Eine Musik-App macht wenige, eher große
//! Anfragen; mehr Threads würden nur Speicher und Aufwachvorgänge kosten.

use std::future::Future;
use std::sync::LazyLock;

static RUNTIME: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("mp-net")
        .build()
        .expect("Tokio-Laufzeit konnte nicht gestartet werden")
});

pub fn block_on<F: Future>(f: F) -> F::Output {
    RUNTIME.block_on(f)
}

/// Wie [`block_on`], aber für Netzwerk-Aufrufe: Fehlermeldungen werden von
/// Anmeldedaten befreit, bevor sie an die Oberfläche gehen.
///
/// WARUM? Die Adressen der Server-Anfragen enthalten Token und Salt als
/// Parameter. Fehlertexte von `reqwest` nennen die volle Adresse
/// ("error sending request for url (https://…?u=…&t=…)"). Würde so ein Text
/// in einem Hinweis angezeigt oder in einem Bildschirmfoto landen, wäre der
/// Zugang zum Server offengelegt.
pub fn block_on_net<T, F: Future<Output = anyhow::Result<T>>>(f: F) -> anyhow::Result<T> {
    RUNTIME.block_on(f).map_err(|e| anyhow::anyhow!(scrub_urls(&format!("{e:#}"))))
}

/// Schneidet bei jeder Adresse im Text alles ab dem `?` ab.
pub fn scrub_urls(text: &str) -> String {
    text.split(' ')
        .map(|word| match (word.find("://"), word.find('?')) {
            (Some(scheme), Some(query)) if query > scheme => {
                // Schließende Klammer o. Ä. am Wortende erhalten.
                let tail: String =
                    word.chars().rev().take_while(|c| matches!(c, ')' | ',' | '.' | ':')).collect();
                format!("{}{}", &word[..query], tail.chars().rev().collect::<String>())
            }
            _ => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_removed_from_error_text() {
        let text = "error sending request for url (https://m.example/rest/ping?u=a&t=geheim&s=salz): timeout";
        let clean = scrub_urls(text);
        assert!(!clean.contains("geheim") && !clean.contains("salz"));
        assert_eq!(clean, "error sending request for url (https://m.example/rest/ping): timeout");
        // Text ohne Adresse bleibt unverändert.
        assert_eq!(scrub_urls("Benutzername oder Passwort falsch"), "Benutzername oder Passwort falsch");
    }
}
