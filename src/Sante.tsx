import { useCallback, useEffect, useState } from "react";
import { api, DemEntree, fmt, Protection, ScanProcessus, ScanSecurite, SvEtat } from "./api";

const LABEL: Record<string, string> = { vert: "À jour", orange: "À vérifier", rouge: "Absent", gris: "Non vérifiable" };
const COLOR: Record<string, string> = { vert: "var(--ok)", orange: "var(--warn)", rouge: "var(--danger)", gris: "var(--muted)" };

function Sauvegarde({ notify }: { notify: (m: string) => void }) {
  const [etats, setEtats] = useState<SvEtat[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const verifier = useCallback(async () => {
    setBusy(true); setError("");
    try { setEtats(await api.sauvegardeEtat()); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  }, [notify]);

  useEffect(() => { verifier(); }, [verifier]);

  const pire = etats?.some((e) => e.statut === "rouge") ? "rouge" : etats?.some((e) => e.statut === "orange") ? "orange" : etats?.some((e) => e.statut === "gris") ? "gris" : "vert";

  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Santé du PC · Sauvegarde</div>
          <div className="muted">Lecture seule : rien n'est créé ni modifié. Les boutons ouvrent les paramètres Windows.</div>
          {etats && <div className="rs-count" style={{ color: COLOR[pire] }}>
            {pire === "vert" ? "Tes sauvegardes sont à jour." : pire === "orange" ? "Certaines sauvegardes sont anciennes ou partielles." : pire === "gris" ? "Certaines sauvegardes ne sont pas vérifiables." : "Une protection manque."}
          </div>}
        </div>
        <button className="btn primary" disabled={busy} onClick={verifier}>{busy ? "Vérification…" : "Revérifier"}</button>
      </div>
      <div className="card" style={{ marginTop: 14 }}>
        {(etats ?? []).map((e) => (
          <div key={e.id} className="rowitem" style={{ cursor: "default" }}>
            <span style={{ color: COLOR[e.statut], fontSize: 18 }}>●</span>
            <div style={{ flex: 1 }}>
              <b>{e.titre}</b> <span className="muted small">· {LABEL[e.statut]}</span>
              <div className="muted small">{e.detail}</div>
            </div>
            {e.reglage && e.statut !== "vert" && <button className="btn" onClick={() => api.sauvegardeOuvrir(e.reglage!).catch((err) => notify(String(err)))}>Ouvrir les paramètres</button>}
          </div>
        ))}
        {error && <div className="rowitem bad">Échec de la vérification : {error}</div>}
        {!etats && !error && <div className="rowitem muted">Vérification…</div>}
      </div>
    </div>
  );
}

const CLASSE: Record<string, [string, string]> = { connu: ["Connu", "var(--ok)"], verifier: ["À vérifier", "var(--warn)"], inconnu: ["Inconnu", "var(--muted)"] };

function Processus({ notify }: { notify: (m: string) => void }) {
  const [seuil, setSeuil] = useState(5);
  const [res, setRes] = useState<ScanProcessus | null>(null);
  const [busy, setBusy] = useState(false);
  const [ouvert, setOuvert] = useState<string | null>(null);
  const [error, setError] = useState("");
  const scan = async () => {
    setBusy(true); setError(""); setRes(null);
    try { setRes(await api.santeProcessus(seuil)); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Processus gourmands</div>
          <div className="muted">Programmes qui utilisent plus de {seuil} % du processeur ou de la mémoire. Observation seule : rien n'est fermé.</div>
          {res && <div className="rs-count">CPU {res.cpu_total.toFixed(0)} % · RAM {fmt(res.ram_utilisee)} / {fmt(res.ram_totale)}</div>}
        </div>
        <div className="row">
          <select value={seuil} disabled={busy} onChange={(e) => { setSeuil(Number(e.target.value)); setRes(null); setOuvert(null); setError(""); }}>
            {[1, 2, 5, 10].map((v) => <option key={v} value={v}>Seuil {v} %</option>)}
          </select>
          <button className="btn primary" disabled={busy} onClick={scan}>{busy ? "Mesure (2 s)…" : "Analyser"}</button>
        </div>
      </div>
      {error && <div className="card rowitem bad">Analyse impossible : {error}. Tu peux réessayer.</div>}
      {res && (
        <div className="card" style={{ marginTop: 14 }}>
          {res.processus.length === 0 && <div className="rowitem muted">Aucun programme au-dessus du seuil.</div>}
          {res.processus.map((p) => {
            const [lib, col] = CLASSE[p.classe];
            const k = p.chemin || p.nom;
            return (
              <div key={k} className="rowitem" onClick={() => setOuvert(ouvert === k ? null : k)} style={{ alignItems: "flex-start" }}>
                <span style={{ color: col, minWidth: 74, fontWeight: 600 }}>{lib}</span>
                <div style={{ flex: 1 }}>
                  <b>{p.nom}</b>{p.instances > 1 && <span className="muted"> × {p.instances}</span>}
                  <div className="muted small">{p.explication}{p.raison && <> <span style={{ color: "var(--warn)" }}>Pourquoi à vérifier : {p.raison}.</span></>}</div>
                  {ouvert === k && <div className="muted small">{p.chemin || "chemin illisible"} · {p.editeur || "éditeur inconnu"} · {p.signe ? "signature valide" : "pas de signature intégrée"}</div>}
                </div>
                <div className="r small">CPU {p.cpu.toFixed(1)} %<br />RAM {fmt(p.ram_octets)} ({p.ram_pct.toFixed(1)} %)</div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ProtectionWin({ notify }: { notify: (m: string) => void }) {
  const [etats, setEtats] = useState<Protection[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const verifier = useCallback(async () => {
    setBusy(true); setError("");
    try { setEtats(await api.santeProtection()); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  }, [notify]);
  useEffect(() => { verifier(); }, [verifier]);
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Protection Windows</div>
          <div className="muted">Defender, pare-feu et mises à jour. DiskLens n'est pas un antivirus : pour une analyse réelle, utilise Microsoft Defender.</div>
        </div>
        <button className="btn primary" disabled={busy} onClick={verifier}>{busy ? "Vérification…" : "Revérifier"}</button>
      </div>
      <div className="card" style={{ marginTop: 14 }}>
        {(etats ?? []).map((e) => (
          <div key={e.id} className="rowitem" style={{ cursor: "default" }}>
            <span style={{ color: COLOR[e.statut], fontSize: 18 }}>●</span>
            <div style={{ flex: 1 }}><b>{e.titre}</b><div className="muted small">{e.detail}</div></div>
            {e.reglage && e.statut !== "vert" && <button className="btn" onClick={() => api.sauvegardeOuvrir(e.reglage!).catch((err) => notify(String(err)))}>Ouvrir</button>}
          </div>
        ))}
        {error && <div className="rowitem bad">Échec de la vérification : {error}</div>}
        {!etats && !error && <div className="rowitem muted">Vérification…</div>}
      </div>
    </div>
  );
}

function Securite({ notify }: { notify: (m: string) => void }) {
  const [res, setRes] = useState<ScanSecurite | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const scan = async () => {
    setBusy(true); setError(""); setRes(null);
    try { setRes(await api.santeSecurite()); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  };
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Intrus : à vérifier</div>
          <div className="muted">Repère les programmes connectés à Internet que DiskLens ne reconnaît pas, et les extensions de navigateur inhabituelles. Aucune suppression depuis cet écran.</div>
        </div>
        <button className="btn primary" disabled={busy} onClick={scan}>{busy ? "Analyse…" : "Analyser"}</button>
      </div>
      {error && <div className="card rowitem bad">Analyse impossible : {error}. Tu peux réessayer.</div>}
      {res && (
        <>
          <p className="muted small">{res.limites.join(" ")}</p>
          <div className="card" style={{ marginTop: 10 }}>
            <header><h3>Connexions de programmes non reconnus ({res.connexions.length})</h3></header>
            {res.connexions.length === 0 && <div className="rowitem muted">Rien à signaler.</div>}
            {res.connexions.map((c, i) => (
              <div key={i} className="rowitem" style={{ cursor: "default" }}>
                <div style={{ flex: 1 }}>
                  <b>{c.processus}</b> <span className="muted small">→ {c.distant}:{c.port}</span>
                  <div className="muted small ellipsis" title={c.chemin}>{c.chemin || "chemin illisible"}</div>
                  {c.raison && <div className="small" style={{ color: "var(--warn)" }}>À vérifier : {c.raison}.</div>}
                </div>
              </div>
            ))}
          </div>
          <div className="card" style={{ marginTop: 14 }}>
            <header><h3>Extensions de navigateur à examiner ({res.extensions.length})</h3></header>
            {res.extensions.length === 0 && <div className="rowitem muted">Rien à signaler.</div>}
            {res.extensions.map((x) => (
              <div key={x.navigateur + x.profil + x.id} className="rowitem" style={{ cursor: "default" }}>
                <div style={{ flex: 1 }}>
                  <b>{x.nom}</b> <span className="muted small">· {x.navigateur} ({x.profil})</span>
                  <div className="small" style={{ color: "var(--warn)" }}>{x.raison || "Accès sensibles demandés"}.</div>
                  {x.permissions_risque.length > 0 && <div className="muted small">Accès : {x.permissions_risque.join(", ")}</div>}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

const SUGG: Record<string, [string, string]> = { garder: ["À garder", "var(--ok)"], facultatif: ["Facultatif", "var(--warn)"], inconnu: ["À juger", "var(--muted)"] };

function Demarrage({ notify }: { notify: (m: string) => void }) {
  const [liste, setListe] = useState<DemEntree[] | null>(null);
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const charger = useCallback(async () => {
    setBusy(true); setError("");
    try { setListe(await api.demarrageListe()); setSel(new Set()); }
    catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  }, [notify]);
  useEffect(() => { charger(); }, [charger]);
  const bascule = (e: DemEntree) => {
    if (e.actif && e.suggestion === "garder") return;
    setSel((s) => { const n = new Set(s); if (n.has(e.id)) n.delete(e.id); else n.add(e.id); return n; });
  };
  const choisis = (liste ?? []).filter((e) => sel.has(e.id));
  const aDesactiver = choisis.filter((e) => e.actif && e.suggestion !== "garder");
  const aReactiver = choisis.filter((e) => !e.actif);
  const appliquer = async (activer: boolean) => {
    const cibles = activer ? aReactiver : aDesactiver;
    if (!cibles.length) return;
    setBusy(true); setError("");
    try {
      const r = await api.demarrageAppliquer(cibles.map((e) => e.id), activer);
      const ko = r.filter((x) => !x.ok);
      notify(`${r.length - ko.length} entrée(s) ${activer ? "réactivée(s)" : "désactivée(s)"}${ko.length ? ` · ${ko.length} refusée(s) : ${ko[0].detail}` : ""}`);
      setListe(await api.demarrageListe()); setSel(new Set());
    } catch (e) { setError(String(e)); notify(String(e)); }
    finally { setBusy(false); }
  };
  const actifs = (liste ?? []).filter((e) => e.actif).length;
  return (
    <div className="page">
      <div className="hero">
        <div>
          <div className="big">Programmes au démarrage</div>
          <div className="muted">Désactiver ne supprime rien et se réactive d'un clic. Les composants marqués « à garder » sont protégés. Prend effet au prochain démarrage de Windows.</div>
          {liste && <div className="rs-count">{actifs} actif(s) sur {liste.length}</div>}
        </div>
        <div className="row">
          <button className="btn" disabled={busy || !aReactiver.length} onClick={() => appliquer(true)}>Réactiver ({aReactiver.length})</button>
          <button className="btn danger" disabled={busy || !aDesactiver.length} onClick={() => appliquer(false)}>Désactiver ({aDesactiver.length})</button>
          <button className="btn" disabled={busy} onClick={charger}>Actualiser</button>
        </div>
      </div>
      <div className="card" style={{ marginTop: 14 }}>
        {(liste ?? []).map((e) => {
          const [lib, col] = SUGG[e.suggestion];
          const protege = e.actif && e.suggestion === "garder";
          return (
            <label key={e.id} className="rowitem" style={{ alignItems: "flex-start", opacity: e.actif ? 1 : 0.6, cursor: protege ? "default" : "pointer" }}>
              <input type="checkbox" checked={sel.has(e.id)} disabled={busy || protege} onChange={() => bascule(e)} />
              <div style={{ flex: 1 }}>
                <b>{e.nom}</b> <span className="muted small">· {e.editeur || "éditeur inconnu"} · {e.source}{e.admin ? " · admin requis" : ""}</span>
                <div className="muted small ellipsis" title={e.commande}>{e.commande}</div>
                {protege && <div className="small" style={{ color: "var(--warn)" }}>Composant protégé : désactivation indisponible.</div>}
                {e.note && <div className="small" style={{ color: "var(--warn)" }}>{e.note}</div>}
              </div>
              <span className="small" style={{ color: col, minWidth: 70 }}>{lib}</span>
              <span className="small" style={{ minWidth: 70, textAlign: "right" }}>{e.actif ? "Actif" : "Désactivé"}</span>
            </label>
          );
        })}
        {error && <div className="rowitem bad">Échec de la lecture ou de l'action : {error}</div>}
        {!liste && !error && <div className="rowitem muted">Lecture…</div>}
      </div>
    </div>
  );
}

export default function Sante({ notify }: { notify: (m: string) => void }) {
  const [sec, setSec] = useState<"processus" | "demarrage" | "securite" | "protection" | "sauvegarde">("processus");
  return (
    <>
      <div className="toolbar">
        <div className="seg">
          <button className={sec === "processus" ? "on" : ""} onClick={() => setSec("processus")}>Processus</button>
          <button className={sec === "demarrage" ? "on" : ""} onClick={() => setSec("demarrage")}>Démarrage</button>
          <button className={sec === "securite" ? "on" : ""} onClick={() => setSec("securite")}>Intrus</button>
          <button className={sec === "protection" ? "on" : ""} onClick={() => setSec("protection")}>Protection</button>
          <button className={sec === "sauvegarde" ? "on" : ""} onClick={() => setSec("sauvegarde")}>Sauvegarde</button>
        </div>
      </div>
      {sec === "processus" && <Processus notify={notify} />}
      {sec === "demarrage" && <Demarrage notify={notify} />}
      {sec === "securite" && <Securite notify={notify} />}
      {sec === "protection" && <ProtectionWin notify={notify} />}
      {sec === "sauvegarde" && <Sauvegarde notify={notify} />}
    </>
  );
}
