import { useCallback, useEffect, useRef, useState } from "react";
import { api, fmt, Logiciel, Residu, CleReg, Lot, QuarResult, RegResult, ResidusReport } from "./api";
import Icon from "./Icon";

type CleRegWithSoftware = CleReg & { logiciel: Logiciel };
const CONFIANCE_LABEL: Record<"sur" | "probable" | "verifier", string> = {
  sur: "Sûr",
  probable: "Probable",
  verifier: "À vérifier",
};

const CONFIANCE_CLASS: Record<"sur" | "probable" | "verifier", string> = {
  sur: "rs-sur",
  probable: "rs-probable",
  verifier: "rs-verifier",
};

export default function Residus({ notify, rapport, onRapport }: { notify: (m: string) => void; rapport: ResidusReport | null; onRapport: (r: ResidusReport) => void }) {
  // Scan state
  const report = rapport, setReport = onRapport;
  const rapportCourant = useRef(report);
  rapportCourant.current = report;
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState("");

  // Registry state
  const [regKeys, setRegKeys] = useState<CleRegWithSoftware[]>([]);
  const [regScanning, setRegScanning] = useState(false);
  const [regError, setRegError] = useState("");
  const [allowRegDelete, setAllowRegDelete] = useState(false);

  // Selection state
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [selectedRegKeys, setSelectedRegKeys] = useState<Set<string>>(new Set());

  // Action state
  const [cleaning, setCleaning] = useState(false);
  const [cleanModalOpen, setCleanModalOpen] = useState(false);
  const [cleanPaths, setCleanPaths] = useState<string[]>([]);
  const [cleanResults, setCleanResults] = useState<QuarResult[] | null>(null);

  const [regDeleting, setRegDeleting] = useState(false);
  const [regModalStep, setRegModalStep] = useState<0 | 1 | 2 | 3>(0);
  const [regKeysToDelete, setRegKeysToDelete] = useState<CleRegWithSoftware[]>([]);
  const [regRestoreResult, setRegRestoreResult] = useState<string | null>(null);
  const [regDeleteResults, setRegDeleteResults] = useState<RegResult[] | null>(null);
  const [regConfirmUnderstood, setRegConfirmUnderstood] = useState(false);

  // Quarantine state
  const [lots, setLots] = useState<Lot[]>([]);
  const [lotsLoading, setLotsLoading] = useState(true);
  const [purgeLot, setPurgeLot] = useState<string | null>(null);

  const [cherche, setCherche] = useState({ nom: "", editeur: "" });
  const [filtreDecouverte, setFiltreDecouverte] = useState("");
  const [vue, setVue] = useState<"fichiers" | "registre" | "quarantaine">("fichiers");
  const [pagePistes, setPagePistes] = useState(0);
  useEffect(() => {
    if (!rapport) return;
    setVue("fichiers"); setSelectedFiles(new Set());
    setRegKeys([]); setSelectedRegKeys(new Set()); setScanError(""); setFiltreDecouverte(""); setPagePistes(0);
  }, [rapport]);

  // --- Scan ---
  const runScan = useCallback(async (manuel = false) => {
    setScanning(true);
    setScanError("");
    try {
      const r = manuel ? await api.residusChercher(cherche.nom, cherche.editeur) : await api.residusScan();
      setReport(r);
      setFiltreDecouverte(""); setPagePistes(0);
      setRegKeys([]);
      setSelectedRegKeys(new Set());
      setSelectedFiles(new Set());
    } catch (e) {
      setScanError(String(e));
      notify(String(e));
    } finally {
      setScanning(false);
    }
  }, [notify, cherche, onRapport]);

  const loadLots = useCallback(async () => {
    setLotsLoading(true);
    try {
      const l = await api.residusLots();
      setLots(l);
    } catch (e) {
      notify(String(e));
    } finally {
      setLotsLoading(false);
    }
  }, [notify]);

  useEffect(() => {
    if (vue === "quarantaine") void loadLots();
  }, [loadLots, vue]);

  // --- Registry scan ---
  const runRegScan = useCallback(async () => {
    if (!report) return;
    setRegScanning(true);
    setRegError("");
    try {
      const fantomes = report.logiciels.filter((l) => l.etat === "fantome");
      const keys = await api.registreScan(fantomes);
      if (rapportCourant.current !== report) return;
      const withLogiciel: CleRegWithSoftware[] = keys.map((k) => {
        const log = report.logiciels.find((l) => l.id === k.logiciel_id);
        return { ...k, logiciel: log! };
      });
      setRegKeys(withLogiciel);
      setSelectedRegKeys(new Set());
    } catch (e) {
      setRegError(String(e));
      notify(String(e));
    } finally {
      setRegScanning(false);
    }
  }, [report, notify]);

  // --- Selection helpers ---
  const toggleFile = useCallback((id: string, residu: Residu) => {
    if (residu.protege) return;
    setSelectedFiles((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  }, []);

  const setManyFiles = useCallback((logicielId: string, on: boolean) => {
    if (!report) return;
    setSelectedFiles((s) => {
      const n = new Set(s);
      report.residus
        .filter((r) => r.logiciel_id === logicielId && !r.protege)
        .forEach((r) => (on ? n.add(r.id) : n.delete(r.id)));
      return n;
    });
  }, [report]);

  const toggleRegKey = useCallback((id: string, key: CleReg) => {
    if (key.protege) return;
    setSelectedRegKeys((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  }, []);

  const setManyRegKeys = useCallback((logicielId: string, on: boolean) => {
    setSelectedRegKeys((s) => {
      const n = new Set(s);
      regKeys
        .filter((k) => k.logiciel_id === logicielId && !k.protege)
        .forEach((k) => (on ? n.add(k.id) : n.delete(k.id)));
      return n;
    });
  }, [regKeys]);

  // --- Clean (quarantine) ---
  const openCleanModal = useCallback(() => {
    const paths: string[] = [];
    if (!report) return;
    report.residus.forEach((r) => {
      if (selectedFiles.has(r.id)) paths.push(r.chemin);
    });
    if (!paths.length) return;
    setCleanPaths(paths);
    setCleanResults(null);
    setCleanModalOpen(true);
  }, [report, selectedFiles]);

  const confirmClean = useCallback(async () => {
    setCleaning(true);
    try {
      const { resultats: res, rapport: nouveau, erreur } = await api.residusNettoyerEtScanner(cleanPaths);
      setCleanResults(res);
      notify(`${res.filter((r) => r.ok).length} élément(s) mis en quarantaine · ${res.filter((r) => !r.ok).length} échec(s)`);
      if (nouveau) setReport(nouveau);
      if (erreur) setScanError(`Les résultats du déplacement sont conservés, mais l’actualisation a échoué : ${erreur}`);
      setSelectedFiles(new Set());
      await loadLots();
    } catch (e) {
      notify(String(e));
    } finally {
      setCleaning(false);
    }
  }, [cleanPaths, notify, loadLots, onRapport]);

  const closeCleanModal = useCallback(() => {
    if (!cleaning) {
      setCleanModalOpen(false);
      setCleanResults(null);
    }
  }, [cleaning]);

  // --- Registry delete ---
  const openRegModal = useCallback(() => {
    const keys = regKeys.filter((k) => selectedRegKeys.has(k.id));
    if (!keys.length) return;
    setRegKeysToDelete(keys);
    setRegModalStep(1);
    setRegConfirmUnderstood(false);
    setRegRestoreResult(null);
    setRegDeleteResults(null);
  }, [selectedRegKeys, regKeys]);

  const confirmRegStep1 = useCallback(() => {
    if (!regConfirmUnderstood) return;
    setRegModalStep(2);
    // Appel à api.registrePointRestauration() - correction 1
    setRegDeleting(true);
    api.registrePointRestauration()
      .then((result) => {
        setRegRestoreResult(result);
        setRegDeleting(false);
      })
      .catch((e) => {
        setRegRestoreResult(`Erreur : ${String(e)}`);
        setRegDeleting(false);
      });
  }, [regConfirmUnderstood]);

  const runRegClean = useCallback(async () => {
    setRegDeleting(true);
    try {
      // Correction 2 : utiliser les chemins (key.chemin) au lieu des IDs
      const chemins = regKeysToDelete.map((k) => k.chemin);
      const { resultats: res, clesRestantes, erreur } = await api.registreNettoyerEtScanner(chemins, report?.logiciels.filter((l) => l.etat === "fantome") ?? []);
      if (rapportCourant.current !== report) {
        setRegModalStep(0); setRegKeysToDelete([]); setRegConfirmUnderstood(false);
        notify(`${res.filter((r) => r.ok).length} clé(s) supprimée(s) pour la sélection précédente · ${res.filter((r) => !r.ok).length} échec(s)`);
        return;
      }
      setRegDeleteResults(res);
      setRegModalStep(3); // Correction 3 : passer à l'étape 3 pour afficher les résultats
      notify(`${res.filter((r) => r.ok).length} clé(s) supprimée(s) · ${res.filter((r) => !r.ok).length} échec(s)`);
      setRegKeys((clesRestantes ?? []).map((k) => ({ ...k, logiciel: report!.logiciels.find((l) => l.id === k.logiciel_id)! })));
      setSelectedRegKeys(new Set());
      if (erreur) setRegError(`Le résultat du nettoyage est conservé, mais l’actualisation a échoué : ${erreur}`);
    } catch (e) {
      notify(String(e));
    } finally {
      setRegDeleting(false);
    }
  }, [regKeysToDelete, report, notify]);

  const closeRegModal = useCallback(() => {
    if (!regDeleting) {
      setRegModalStep(0);
      setRegKeysToDelete([]);
      setRegConfirmUnderstood(false);
      setRegRestoreResult(null);
      setRegDeleteResults(null);
    }
  }, [regDeleting]);

  // --- Purge lot ---
  const confirmPurgeLot = useCallback(async (lotName: string) => {
    try {
      await api.residusPurgeLot(lotName);
      notify(`Lot ${lotName} purgé`);
      await loadLots();
    } catch (e) {
      notify(String(e));
    }
  }, [notify, loadLots]);

  // --- Computed ---
  const ghostLogiciels = report?.logiciels.filter((l) => l.etat === "fantome") ?? [];
  // Correction 5 : ne garder que les logiciels fantômes qui ont au moins un résidu
  const ghostLogicielsAvecResidus = ghostLogiciels.filter((log) => {
    const residues = report?.residus.filter((r) => r.logiciel_id === log.id) ?? [];
    return residues.length > 0;
  });
  const selectedFilesCount = selectedFiles.size;
  const selectedFilesSize = report?.residus
    .filter((r) => selectedFiles.has(r.id))
    .reduce((s, r) => s + r.octets, 0) ?? 0;
  const selectedRegCount = selectedRegKeys.size;
  const pistes = report?.decouverte?.dossiers ?? [];
  const pistesVisibles = pistes.filter((p) => `${p.nom} ${p.chemin}`.toLocaleLowerCase().includes(filtreDecouverte.toLocaleLowerCase()));

  const residuesByLogiciel = new Map<string, Residu[]>();
  report?.residus.forEach((r) => {
    if (!residuesByLogiciel.has(r.logiciel_id)) residuesByLogiciel.set(r.logiciel_id, []);
    residuesByLogiciel.get(r.logiciel_id)!.push(r);
  });

  const regKeysByLogiciel = new Map<string, CleRegWithSoftware[]>();
  regKeys.forEach((k) => {
    if (!regKeysByLogiciel.has(k.logiciel_id)) regKeysByLogiciel.set(k.logiciel_id, []);
    regKeysByLogiciel.get(k.logiciel_id)!.push(k);
  });

  const nombrePages = Math.max(1, Math.ceil(pistesVisibles.length / 30));
  const pageCourante = Math.min(pagePistes, nombrePages - 1);
  const pistesAffichees = pistesVisibles.slice(pageCourante * 30, (pageCourante + 1) * 30);

  // --- Render ---
  return (
    <div className="page rs-page">
      <section className="page-heading"><div><h2>Les traces laissées par vos logiciels</h2><p className="muted">Recherchez les dossiers restants, examinez les résultats, puis choisissez le nettoyage.</p></div>{vue === "fichiers" && <button className="btn primary" disabled={scanning} onClick={() => runScan()}><Icon name="search" size={18} />{scanning ? "Recherche…" : "Rechercher les résidus"}</button>}</section>
      <div className="view-switcher" aria-label="Vues des résidus">
        <button aria-pressed={vue === "fichiers"} className={vue === "fichiers" ? "active" : ""} onClick={() => setVue("fichiers")}><Icon name="residue" size={18} />Fichiers et dossiers</button>
        <button aria-pressed={vue === "registre"} className={vue === "registre" ? "active" : ""} onClick={() => setVue("registre")}><Icon name="registry" size={18} />Registre<span className="small muted">Avancé</span></button>
        <button aria-pressed={vue === "quarantaine"} className={vue === "quarantaine" ? "active" : ""} onClick={() => setVue("quarantaine")}><Icon name="archive" size={18} />Quarantaine</button>
      </div>
      {scanError && <div className="inline-message error" role="alert">{scanError}</div>}
      <div hidden={vue !== "fichiers"} className="rs-file-view">
      {report && !report.decouverte && report.logiciels.length === 1 && <div className="inline-message"><Icon name="package" /><div><strong>Restes de {report.logiciels[0].nom}</strong><p>La recherche ciblée est terminée. Examinez les chemins et les tailles avant de cocher les dossiers à mettre en quarantaine.</p><a href="#logiciels">Revenir aux logiciels</a></div></div>}
      <details className="manual-disclosure"><summary>Je connais le nom d’un ancien logiciel</summary>
      <form className="card rs-manual" onSubmit={(e) => { e.preventDefault(); void runScan(true); }}>
        <header><h3>Chercher les restes d'un logiciel déjà désinstallé</h3></header>
        <div className="row rs-manual-row">
          <label className="grow">Nom du logiciel<input value={cherche.nom} placeholder="Ex. : Revit 2024" onChange={(e) => setCherche({ ...cherche, nom: e.target.value })} /></label>
          <label className="grow">Éditeur (facultatif)<input value={cherche.editeur} placeholder="Ex. : Autodesk" onChange={(e) => setCherche({ ...cherche, editeur: e.target.value })} /></label>
          <button className="btn primary" disabled={scanning || cherche.nom.trim().length < 4}>Chercher</button>
        </div>
      </form></details>

      {report?.decouverte && (
        <section className="card rs-card">
          <header><h3>Résidus possibles trouvés automatiquement ({pistes.length})</h3><span className="pill rs-verifier">À vérifier · lecture seule</span></header>
          <p className="muted small" style={{ padding: "0 14px" }}>
            Aucun nom à saisir : recherche dans AppData, ProgramData et les dossiers de programmes, avec recoupement des noms, éditeurs, emplacements et processus actifs.
            L'absence d'inscription ne prouve pas une désinstallation : une application portable ou du Store peut être encore utilisée. Les noms sont déduits des dossiers.
            Ces pistes ne sont pas sélectionnées pour le nettoyage.
          </p>
          {pistes.length > 0 && <label className="row" style={{ padding: "0 14px 10px" }}>Filtrer les dossiers
            <input className="grow" value={filtreDecouverte} onChange={(e) => { setFiltreDecouverte(e.target.value); setPagePistes(0); }} placeholder="Nom supposé ou chemin…" />
          </label>}
          {pistesAffichees.map((p) => (
            <div className="rowitem rs-rowitem" key={p.chemin}>
              <div className="grow">
                <strong>{p.nom}</strong>
                <div className="rs-path" style={{ overflowWrap: "anywhere" }}>{p.chemin}</div>
                {p.indices.map((indice, i) => <div className="muted small" key={i}>{indice}</div>)}
                <div className="muted small">{p.fichiers} fichier(s) mesuré(s){p.partiel ? " · Mesure partielle : taille totale inconnue" : " · Taille logique, gain disque non garanti"}</div>
              </div>
              <b className="r rs-size">{p.partiel ? `≥ ${fmt(p.octets)}` : fmt(p.octets)}</b>
              <button className="btn" onClick={() => void api.reveal(p.chemin).catch((e) => notify(String(e)))}>Voir le dossier</button>
            </div>
          ))}
          {pistesVisibles.length === 0 && <div className="empty"><p>{pistes.length ? "Aucun dossier ne correspond à ce filtre." : "Aucun dossier supplémentaire repéré dans les emplacements analysés."}</p></div>}
          {pistesVisibles.length > 30 && <div className="pagination"><span className="muted small">{pageCourante * 30 + 1}–{Math.min((pageCourante + 1) * 30, pistesVisibles.length)} sur {pistesVisibles.length} dossiers</span><div className="row"><button className="btn" disabled={pageCourante === 0} onClick={() => setPagePistes(pageCourante - 1)}>Précédent</button><span className="small">{pageCourante + 1} / {nombrePages}</span><button className="btn" disabled={pageCourante + 1 >= nombrePages} onClick={() => setPagePistes(pageCourante + 1)}>Suivant</button></div></div>}
          <details style={{ padding: "10px 14px" }}>
            <summary>Couverture et limites de la recherche · {report.decouverte.dossiers_examines} dossier(s) candidat(s) examiné(s)</summary>
            <p className="muted small">Recherche limitée aux dossiers immédiats et aux produits millésimés chez les éditeurs installés. Dossiers système et partagés exclus. Liens, jonctions et fichiers cloud ignorés. Aucun parcours de vos documents ou de OneDrive. Les autres profils et installations personnalisées ne sont pas couverts entièrement.</p>
            {report.decouverte.limites.map((limite, i) => <p className="muted small" key={i}>{limite}</p>)}
          </details>
        </section>
      )}

      {/* Liste des logiciels fantômes */}
      {ghostLogicielsAvecResidus.length === 0 ? (
        <div className="empty">
          <h2>{report ? "Aucun dossier lié à une ancienne inscription" : "Retrouver les traces d’anciens logiciels"}</h2>
          <p>{report ? "Aucun dossier associé aux inscriptions de désinstallation fantômes n'a été trouvé. Les pistes automatiques éventuelles sont présentées plus haut." : "Cliquez sur Rechercher les résidus. Aucun nom à saisir et aucune suppression automatique."}</p>
        </div>
      ) : (
        <>
          {ghostLogicielsAvecResidus.map((log) => {
            const residues = residuesByLogiciel.get(log.id) ?? [];
            const allChecked = residues.length > 0 && residues.every((r) => selectedFiles.has(r.id) || r.protege);
            const someChecked = residues.some((r) => selectedFiles.has(r.id));
            return (
              <section key={log.id} className="card rs-card">
                <header>
                  <h3>{log.nom}</h3>
                  <span className="muted">{log.editeur} · v{log.version}</span>
                  <span className="muted small">({log.raison})</span>
                  <button className="link" onClick={() => setManyFiles(log.id, !allChecked)}>
                    {allChecked ? "Tout décocher" : someChecked ? "Tout cocher" : "Tout cocher / décocher"}
                  </button>
                </header>
                {residues.map((res) => (
                  <label key={res.id} className="rowitem rs-rowitem">
                    <input type="checkbox" checked={selectedFiles.has(res.id)} onChange={() => toggleFile(res.id, res)} disabled={res.protege !== null} aria-label={`Sélectionner ${res.chemin}`} />
                    <div className="grow">
                      <div className="ellipsis rs-path" title={res.chemin}>{res.chemin}</div>
                      <div className="muted small rs-motif">{res.motif}</div>
                    </div>
                    <span className={`pill ${CONFIANCE_CLASS[res.confiance]} rs-confiance`}>{CONFIANCE_LABEL[res.confiance]}</span>
                    <b className="r rs-size">{fmt(res.octets)}</b>
                    {/* Correction 6 : afficher la raison de protection en rouge directement */}
                    {res.protege && <span className="r rs-protege" style={{ color: "var(--danger)" }}>{res.protege}</span>}
                  </label>
                ))}
              </section>
            );
          })}
        </>
      )}

      </div>
      {/* Registre */}
      <section hidden={vue !== "registre"} className="card rs-card rs-reg-section">
        <header>
          <h3>Registre</h3>
          <button className="btn" disabled={regScanning || !ghostLogiciels.length} onClick={runRegScan}>
            {regScanning ? <><span className="pulse" /> Recherche…</> : "Chercher dans le registre (lecture seule)"}
          </button>
        </header>

        {!report && <div className="inline-message"><span>Recherchez d’abord les résidus pour associer les clés à un logiciel.</span><button className="btn" disabled={scanning} onClick={() => runScan()}>{scanning ? "Recherche…" : "Analyser les résidus"}</button></div>}
        <p className="muted small section-body">L’analyse est en lecture seule. La suppression des clés reste une action avancée, avec export et confirmation séparés.</p>
        {regError && <div className="rowitem bad">{regError}</div>}

        <label className="rowitem rs-reg-toggle" style={{ cursor: "default" }}>
          <input type="checkbox" checked={allowRegDelete} onChange={(e) => setAllowRegDelete(e.target.checked)} disabled={regScanning} />
          <div className="grow">
            <div>Autoriser la suppression du registre</div>
            <div className="muted small">Exporte les clés en .reg dans la quarantaine avant suppression</div>
          </div>
        </label>

        {!allowRegDelete && regKeys.length > 0 && (
          <div className="warn rs-warn" style={{ margin: "8px 14px", padding: "8px 12px" }}>
            ⚠ Mode lecture seule — active l'interrupteur pour autoriser la suppression.
          </div>
        )}

        {regKeys.length > 0 && report && (
                  <>
                    {Array.from(regKeysByLogiciel.entries()).map(([logicielId, keys]) => {
                      const log = report.logiciels.find((l) => l.id === logicielId);
              const allChecked = keys.length > 0 && keys.every((k) => selectedRegKeys.has(k.id) || k.protege);
              const someChecked = keys.some((k) => selectedRegKeys.has(k.id));
              return (
                <div key={logicielId} className="rs-reg-group">
                  <div className="card-header rs-reg-header" style={{ padding: "10px 14px", borderBottom: "1px solid var(--line)", background: "var(--panel2)" }}>
                    <strong>{log?.nom ?? logicielId}</strong>
                    <span className="muted small"> · {keys.length} clé(s)</span>
                    <button className="link" disabled={!allowRegDelete} onClick={() => setManyRegKeys(logicielId, !allChecked)}>
                      {allChecked ? "Tout décocher" : someChecked ? "Tout cocher" : "Tout cocher / décocher"}
                    </button>
                  </div>
                  {keys.map((key) => (
                    <label key={key.id} className="rowitem rs-rowitem rs-reg-row">
                      <input type="checkbox" checked={selectedRegKeys.has(key.id)} onChange={() => toggleRegKey(key.id, key)} disabled={!allowRegDelete || key.protege !== null} aria-label={`Sélectionner clé ${key.chemin}`} />
                      <div className="grow">
                        <div className="ellipsis rs-path" title={key.chemin}>{key.chemin}</div>
                        <div className="muted small rs-motif">{key.genre} · {key.valeurs} valeur(s) · {key.motif}</div>
                      </div>
                      <span className={`pill ${CONFIANCE_CLASS[key.confiance]} rs-confiance`}>{CONFIANCE_LABEL[key.confiance]}</span>
                      {/* Correction 6 : afficher la raison de protection en rouge directement */}
                      {key.protege && <span className="r rs-protege" style={{ color: "var(--danger)" }}>{key.protege}</span>}
                    </label>
                  ))}
                </div>
              );
            })}
          </>
        )}

        {regKeys.length === 0 && !regScanning && (
          <div className="empty" style={{ padding: "20px 14px" }}>
            <p className="muted">Aucune clé de registre trouvée. Lance la recherche.</p>
          </div>
        )}
      </section>

      {/* Barre d'action sticky */}
      <div className="rs-action-bar" hidden={vue === "quarantaine" || (vue === "fichiers" ? selectedFilesCount === 0 : selectedRegCount === 0 || !allowRegDelete)}>
        {vue === "fichiers" && selectedFilesCount > 0 && (
          <button className="btn primary rs-btn-quarantine" disabled={cleaning} onClick={openCleanModal}>
            Mettre en quarantaine ({selectedFilesCount} dossiers · {fmt(selectedFilesSize)})
          </button>
        )}
        {vue === "registre" && allowRegDelete && selectedRegCount > 0 && (
          <button className="btn danger rs-btn-regdelete" disabled={regDeleting} onClick={openRegModal}>
            Supprimer {selectedRegCount} clé{selectedRegCount > 1 ? "s" : ""} du registre
          </button>
        )}
      </div>

      {/* Quarantaine - Correction 4 : toujours afficher la section, même sans report */}
      <section hidden={vue !== "quarantaine"} className="card rs-card rs-quarantine-section">
        <header><h3>Lots conservés en quarantaine</h3><button className="btn" disabled={lotsLoading} onClick={loadLots}><Icon name="refresh" size={16} />Actualiser</button></header>
        <p className="muted section-body">Les lots conservent les fichiers retirés. Une purge est définitive et demande une confirmation. Un déplacement sur le même disque ne libère pas d’espace avant cette purge.</p>
        {lotsLoading ? (
          <div className="rowitem"><span className="pulse" /> Chargement…</div>
        ) : lots.length === 0 ? (
          <div className="empty" style={{ padding: "20px 14px" }}><p className="muted">Aucun lot en quarantaine.</p></div>
        ) : (
          <>
            {lots.map((lot) => (
              <label key={lot.nom} className="rowitem rs-lot-row" style={{ cursor: "default" }}>
                <div className="grow">
                  <div className="ellipsis rs-lot-name" title={lot.nom}>
                    {lot.nom} {lot.age_jours > 30 && <span className="pill rs-old-lot"> {" > 30 j"} </span>}
                  </div>
                  <div className="muted small rs-lot-meta">{lot.age_jours} jour{lot.age_jours > 1 ? "s" : ""} · {fmt(lot.octets)}</div>
                </div>
                <button className="btn danger rs-btn-purge" disabled={purgeLot === lot.nom} onClick={() => setPurgeLot(lot.nom)}>
                  {purgeLot === lot.nom ? "Suppression…" : "Supprimer définitivement"}
                </button>
              </label>
            ))}
          </>
        )}
      </section>

      {/* Clean Modal */}
      {cleanModalOpen && (
        <div className="overlay" onClick={() => !cleaning && closeCleanModal()}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>{cleanResults ? "Terminé" : "Mise en quarantaine"}</h2>
            {!cleanResults && (
              <p className="warn">⚠ Ces éléments seront déplacés vers la quarantaine (robocopy /MOVE). Un journal <code>manifeste.json</code> est créé dans le lot.</p>
            )}
            <div className="list-box">
              {(cleanResults ?? cleanPaths.map((p) => ({ chemin: p, ok: true, detail: "", lot: "" }))).map((r, i) => (
                <div key={i} className="rowitem" style={{ cursor: "default" }}>
                  <div className="grow">
                    <div className="ellipsis" title={r.chemin || cleanPaths[i]}>{r.chemin || cleanPaths[i]}</div>
                    {r.detail && <div className="small">{r.detail}</div>}
                  </div>
                  {cleanResults ? <span>{r.ok ? "✅" : "❌"}</span> : <span className="pulse" />}
                </div>
              ))}
            </div>
            <div className="row between">
              {cleanResults ? (
                <b>{cleanResults.filter((r) => r.ok).length} mis en quarantaine · {cleanResults.filter((r) => !r.ok).length} échec(s)</b>
              ) : (
                <b>{cleanPaths.length} élément(s)</b>
              )}
              <div className="row">
                <button className="btn" disabled={cleaning} onClick={closeCleanModal}>{cleanResults ? "Fermer" : "Annuler"}</button>
                {!cleanResults && <button className="btn primary" disabled={cleaning} onClick={confirmClean}>{cleaning ? "Mise en quarantaine…" : "Confirmer"}</button>}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Registry Delete Modal */}
      {regModalStep > 0 && (
        <div className="overlay" onClick={() => !regDeleting && closeRegModal()}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            {regModalStep === 1 && (
              <>
                <h2>Suppression de clés du registre</h2>
                <p className="warn">⚠ Les clés sélectionnées seront exportées en <code>.reg</code> dans la quarantaine, puis supprimées.</p>
                <label className="rowitem" style={{ cursor: "default", marginTop: "8px" }}>
                  <input type="checkbox" checked={regConfirmUnderstood} onChange={(e) => setRegConfirmUnderstood(e.target.checked)} />
                  <div className="grow">Je comprends que les clés seront exportées en <code>.reg</code> dans la quarantaine puis supprimées.</div>
                </label>
                <div className="row between">
                  <button className="btn" onClick={closeRegModal}>Annuler</button>
                  <button className="btn danger" disabled={!regConfirmUnderstood} onClick={confirmRegStep1}>Continuer → Point de restauration</button>
                </div>
              </>
            )}
            {regModalStep === 2 && (
              <>
                <h2>Point de restauration système</h2>
                {regRestoreResult ? (
                  <>
                    <div className="list-box" style={{ maxHeight: "200px" }}>
                      <div className="rowitem"><div className="grow"><div className="ellipsis" title={regRestoreResult}>{regRestoreResult}</div></div></div>
                    </div>
                    {(regRestoreResult.toLowerCase().includes("administrateur") || regRestoreResult.toLowerCase().includes("droits")) && (
                      <div className="warn rs-warn">⚠ Droits administrateur requis pour créer le point de restauration. Tu peux arrêter ici ou continuer (la suppression échouera sans droits admin).</div>
                    )}
                    <div className="row between">
                      <button className="btn" disabled={regDeleting} onClick={closeRegModal}>Arrêter</button>
                      <button className="btn danger" disabled={regDeleting} onClick={runRegClean}>{regDeleting ? "Suppression…" : "Continuer la suppression"}</button>
                    </div>
                  </>
                ) : (
                  <div className="rowitem"><span className="pulse" /> Création du point de restauration…</div>
                )}
              </>
            )}
            {regModalStep === 3 && regDeleteResults && (
              <>
                <h2>Terminé</h2>
                <div className="list-box">
                  {regDeleteResults.map((r, i) => (
                    <div key={i} className="rowitem" style={{ cursor: "default" }}>
                      <div className="grow">
                        <div className="ellipsis" title={r.chemin}>{r.chemin}</div>
                        {r.detail && <div className="small">{r.detail}</div>}
                        {r.export && <div className="small muted">Exporté : {r.export}</div>}
                      </div>
                      <span>{r.ok ? "✅" : "❌"}</span>
                    </div>
                  ))}
                </div>
                <div className="row between">
                  <b>{regDeleteResults.filter((r) => r.ok).length} supprimée(s) · {regDeleteResults.filter((r) => !r.ok).length} échec(s)</b>
                  <button className="btn" onClick={closeRegModal}>Fermer</button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Purge Lot Modal */}
      {purgeLot && (
        <div className="overlay" onClick={() => setPurgeLot(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Supprimer définitivement le lot</h2>
            <p className="warn">⚠ Cette action supprime <b>définitivement</b> le lot <code>{purgeLot}</code> de la quarantaine. Aucune restauration possible.</p>
            <div className="row between">
              <button className="btn" onClick={() => setPurgeLot(null)}>Annuler</button>
              <button className="btn danger" onClick={() => { const name = purgeLot!; setPurgeLot(null); confirmPurgeLot(name); }}>Supprimer définitivement</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
