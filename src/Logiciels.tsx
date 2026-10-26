import { useCallback, useEffect, useMemo, useState } from "react";
import { api, Logiciel, ResidusReport } from "./api";
import Icon from "./Icon";
import Dialog from "./Dialog";

type Suivi = { id: string; nom: string };
type Filtre = "tous" | "anciens" | "inconnus";
function lireSuivi(): Suivi | null {
  try {
    const s = JSON.parse(sessionStorage.getItem("disklens-desinstallation") ?? "null");
    return s && typeof s.id === "string" && typeof s.nom === "string" ? { id: s.id, nom: s.nom } : null;
  } catch { return null; }
}
function dateFr(s: string) {
  const d = new Date(`${s}T12:00:00Z`);
  return Number.isFinite(d.getTime()) ? new Intl.DateTimeFormat("fr-FR", { dateStyle: "medium", timeZone: "UTC" }).format(d) : "Date inconnue";
}
const normaliser = (s: string) => s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLocaleLowerCase("fr");

export default function Logiciels({ notify, onResidus, actif = true }: { notify: (s: string) => void; onResidus: (r: ResidusReport) => void; actif?: boolean }) {
  const [liste, setListe] = useState<Logiciel[] | null>(null);
  const [chargement, setChargement] = useState(true);
  const [erreur, setErreur] = useState("");
  const [recherche, setRecherche] = useState("");
  const [filtre, setFiltre] = useState<Filtre>("tous");
  const [page, setPage] = useState(0);
  const [confirmation, setConfirmation] = useState<Logiciel | null>(null);
  const [action, setAction] = useState(false);
  const [erreurAction, setErreurAction] = useState("");
  const [suivi, setSuivi] = useState<Suivi | null>(lireSuivi);
  useEffect(() => { if (!actif) setConfirmation(null); }, [actif]);
  const charger = useCallback(async () => {
    setChargement(true); setErreur("");
    try { setListe(await api.logicielsListe()); }
    catch (e) { setErreur(String(e)); }
    finally { setChargement(false); }
  }, []);
  useEffect(() => { void charger(); }, [charger]);
  useEffect(() => {
    try {
      if (suivi) sessionStorage.setItem("disklens-desinstallation", JSON.stringify(suivi));
      else sessionStorage.removeItem("disklens-desinstallation");
    } catch { /* Suivi disponible en mémoire. */ }
  }, [suivi]);

  const lancer = async () => {
    if (!confirmation) return;
    setAction(true); setErreurAction("");
    try {
      const message = await api.logicielDesinstaller(confirmation.id, true);
      setSuivi({ id: confirmation.id, nom: confirmation.nom });
      setConfirmation(null); notify(message);
    } catch (e) { setErreurAction(String(e)); }
    finally { setAction(false); }
  };
  const verifier = async () => {
    if (!suivi) return;
    setAction(true); setErreurAction("");
    try {
      const r = await api.logicielResidusApres(suivi.id);
      setSuivi(null); onResidus(r); void charger();
      notify(`Recherche terminée pour ${suivi.nom}. Examinez ses restes avant de les sélectionner.`);
    } catch (e) { setErreurAction(String(e)); }
    finally { setAction(false); }
  };

  const donnees = useMemo(() => {
    const limite = new Date(); limite.setMonth(limite.getMonth() - 6);
    const installes = (liste ?? []).filter((l) => l.etat === "installe");
    const ancien = (l: Logiciel) => !!l.derniere_utilisation_estimee && new Date(`${l.derniere_utilisation_estimee}T00:00:00`) < limite;
    const comptes = { tous: installes.length, anciens: installes.filter(ancien).length, inconnus: installes.filter((l) => !l.derniere_utilisation_estimee).length };
    const termes = normaliser(recherche).trim().split(/\s+/).filter(Boolean);
    const visibles = installes.filter((l) => (filtre === "tous" || (filtre === "anciens" ? ancien(l) : !l.derniere_utilisation_estimee)) && termes.every((t) => normaliser(`${l.nom} ${l.editeur}`).includes(t)))
      .sort((a, b) => a.nom.localeCompare(b.nom, "fr", { sensitivity: "base", numeric: true }));
    return { comptes, visibles };
  }, [liste, recherche, filtre]);
  const pages = Math.max(1, Math.ceil(donnees.visibles.length / 30));
  const index = Math.min(page, pages - 1);
  const lignes = donnees.visibles.slice(index * 30, (index + 1) * 30);
  return <div className="page software-page">
    <section className="page-heading"><div><h2>Vos logiciels, au même endroit</h2><p className="muted">Recherchez une application et lancez son désinstalleur officiel. DiskLens vous accompagne ensuite pour examiner les restes.</p></div><button className="btn" onClick={charger} disabled={chargement || action}><Icon name="refresh" size={16} />{chargement ? "Chargement…" : "Actualiser"}</button></section>
    {suivi && <section className="workflow-card"><div className="workflow-top"><span className="task-icon"><Icon name="package" /></span><div className="grow"><h3>Désinstallation de {suivi.nom}</h3><p className="muted">Terminez les étapes dans la fenêtre du désinstalleur, puis revenez ici.</p></div><button className="icon-button" aria-label="Fermer le suivi de désinstallation" disabled={action} onClick={() => { setSuivi(null); setErreurAction(""); }}><Icon name="close" /></button></div><ol className="workflow-steps" aria-label="Étapes de la désinstallation"><li className="done">Désinstalleur ouvert</li><li className="current" aria-current="step">Vérifier les restes</li><li>Choisir le nettoyage</li></ol>{erreurAction && <div className="inline-message error" role="alert">{erreurAction}</div>}<div className="workflow-footer"><span className="muted small">Aucun reste ne sera supprimé automatiquement.</span><button className="btn primary" disabled={action} onClick={verifier}>{action ? "Vérification…" : "Vérifier et chercher les restes"}<Icon name="arrow" size={16} /></button></div></section>}
    <section className="card software-list">
      <div className="software-toolbar"><label className="search-field"><Icon name="search" size={18} /><span className="sr-only">Rechercher un logiciel ou un éditeur</span><input type="search" name="logiciel" autoComplete="off" spellCheck={false} value={recherche} onChange={(e) => { setRecherche(e.target.value); setPage(0); }} placeholder="Rechercher un logiciel ou un éditeur…" /></label><span className="muted small" aria-live="polite">{liste ? `${donnees.comptes.tous} logiciels enregistrés` : "Inventaire en cours…"}</span></div>
      <div className="filter-bar" aria-label="Filtrer les logiciels">{([['tous', 'Tous'], ['anciens', 'Dernière trace > 6 mois'], ['inconnus', 'Usage inconnu']] as [Filtre, string][]).map(([id, label]) => <button className={`filter-button${filtre === id ? " active" : ""}`} key={id} aria-pressed={filtre === id} onClick={() => { setFiltre(id); setPage(0); }}>{label}<span>{donnees.comptes[id]}</span></button>)}</div>
      {erreur && <div className="inline-message error" role="alert">Inventaire indisponible : {erreur}. Réessayez avec Actualiser.{liste && " Les résultats affichés sont ceux du dernier inventaire."}</div>}
      {!liste && chargement ? <div className="loading-state" role="status"><span className="pulse" />Lecture des logiciels enregistrés, sans recherche de résidus…</div> : <>
        <div className="software-columns" aria-hidden="true"><span>Application</span><span>Dernière trace de lancement</span><span>Action</span></div>
        {lignes.map((l) => <div className="software-row" key={l.id}><div className="software-identity"><span className="software-symbol"><Icon name="package" size={20} /></span><div className="grow"><strong>{l.nom}</strong><div className="muted small">{l.editeur || "Éditeur non renseigné"}{l.version ? ` · ${l.version}` : ""}</div><details className="software-details"><summary>Informations</summary><div><p>Installation déclarée : {l.date_installation ? dateFr(l.date_installation) : "date inconnue"}</p>{l.install_location && <p className="path-text">{l.install_location}</p>}<p>{l.indice_usage || "L’absence de trace ne prouve pas que ce logiciel n’est pas utilisé."}</p>{l.origine_oem && <p>Origine constructeur possible : {l.origine_oem} ({l.confiance_oem ?? "inconnue"}).</p>}</div></details></div></div><div className="software-usage">{l.derniere_utilisation_estimee ? <><span>{dateFr(l.derniere_utilisation_estimee)}</span><small className="muted">Estimation UserAssist</small></> : <span className="status-label">Usage inconnu</span>}</div><div className="software-action"><button className="btn" disabled={!l.desinstalleur || action || suivi !== null} aria-label={`Désinstaller ${l.nom}`} onClick={() => { setConfirmation(l); setErreurAction(""); }}>Désinstaller…</button>{!l.desinstalleur && <small className="muted">À gérer dans Windows</small>}</div></div>)}
        {!lignes.length && !chargement && !erreur && <div className="empty compact"><Icon name="search" size={28} /><h3>{recherche || filtre !== "tous" ? "Aucun logiciel ne correspond" : "Aucun logiciel enregistré trouvé"}</h3><p>{recherche || filtre !== "tous" ? "Essayez un autre nom ou revenez au filtre Tous." : "Les applications portables et certaines applications du Store peuvent ne pas figurer ici."}</p></div>}
        {donnees.visibles.length > 0 && <div className="pagination"><span className="muted small" aria-live="polite">{index * 30 + 1}–{Math.min((index + 1) * 30, donnees.visibles.length)} sur {donnees.visibles.length}</span><div className="row"><button className="btn" disabled={index === 0} onClick={() => setPage(index - 1)}>Précédent</button><span className="small">{index + 1} / {pages}</span><button className="btn" disabled={index + 1 >= pages} onClick={() => setPage(index + 1)}>Suivant</button></div></div>}
      </>}
    </section>
    <p className="page-footnote"><Icon name="shield" size={16} />Une absence de trace ne signifie pas une absence d’utilisation. Le désinstalleur peut demander les droits administrateur ou un redémarrage.</p>
    {actif && confirmation && <Dialog title={`Désinstaller ${confirmation.nom} ?`} busy={action} onClose={() => setConfirmation(null)}><p>Le désinstalleur officiel s’ouvrira dans sa propre fenêtre. Suivez ses étapes pour confirmer ou annuler la désinstallation.</p><div className="inline-message"><Icon name="residue" /><span>Ensuite, DiskLens vérifiera le résultat et vous montrera les restes à examiner. Leur nettoyage demandera une confirmation séparée.</span></div>{erreurAction && <div className="inline-message error" role="alert">{erreurAction}</div>}<div className="row between"><button className="btn" disabled={action} onClick={() => setConfirmation(null)}>Annuler</button><button className="btn danger" disabled={action} onClick={lancer}>{action ? "Ouverture…" : "Ouvrir le désinstalleur"}</button></div></Dialog>}
  </div>;
}
