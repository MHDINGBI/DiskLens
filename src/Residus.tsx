import { useCallback, useEffect, useRef, useState } from "react";
import { api, fmt, Logiciel, Residu, CleReg, Lot, QuarResult, RegResult, ResidusReport } from "./api";

type CleRegWithSoftware = CleReg & { logiciel: Logiciel };
type SuiviDesinstallation = { id: string; nom: string };
function lireSuivi(): SuiviDesinstallation | null {
  try {
    const s = JSON.parse(sessionStorage.getItem("disklens-desinstallation") ?? "null");
    return s && typeof s.id === "string" && typeof s.nom === "string" ? { id: s.id, nom: s.nom } : null;
  } catch { return null; }
}
type LogicielUsage = Logiciel & {
  date_installation?: string | null;
  derniere_utilisation_estimee?: string | null;
  confiance_usage?: string;
  indice_usage?: string;
  origine_oem?: string | null;
  confiance_oem?: string;
};

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

export default function Residus({ notify }: { notify: (m: string) => void }) {
  // Scan state
  const [report, setReport] = useState<ResidusReport | null>(null);
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
  const [filtreLogiciels, setFiltreLogiciels] = useState<"tous" | "anciens" | "inconnus">("tous");
  const [desinstallation, setDesinstallation] = useState<LogicielUsage | null>(null);
  const [desinstallationBusy, setDesinstallationBusy] = useState(false);
  const [suivi, setSuivi] = useState<SuiviDesinstallation | null>(lireSuivi);
  const [suiviVerifie, setSuiviVerifie] = useState(false);
  const [suiviErreur, setSuiviErreur] = useState("");
  const [filtreNomLogiciel, setFiltreNomLogiciel] = useState("");

  useEffect(() => {
    try {
      if (suivi) sessionStorage.setItem("disklens-desinstallation", JSON.stringify(suivi));
      else sessionStorage.removeItem("disklens-desinstallation");
    } catch { /* Le suivi reste disponible en mémoire si le stockage de session est indisponible. */ }
  }, [suivi]);

  // --- Scan ---
  const runScan = useCallback(async (manuel = false) => {
    setScanning(true);
    setScanError("");
    try {
      const r = manuel ? await api.residusChercher(cherche.nom, cherche.editeur) : await api.residusScan();
      setReport(r);
      setFiltreDecouverte("");
      setRegKeys([]);
      setSelectedRegKeys(new Set());
      setSelectedFiles(new Set());
      loadLots();
    } catch (e) {
      setScanError(String(e));
      notify(String(e));
    } finally {
      setScanning(false);
    }
  }, [notify, cherche]);

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
    loadLots();
  }, [loadLots]);

  // --- Registry scan ---
  const runRegScan = useCallback(async () => {
    if (!report) return;
    setRegScanning(true);
    setRegError("");
    try {
      const fantomes = report.logiciels.filter((l) => l.etat === "fantome");
      const keys = await api.registreScan(fantomes);
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
      const res = await api.residusClean(cleanPaths);
      setCleanResults(res);
      notify(`${res.filter((r) => r.ok).length} élément(s) mis en quarantaine · ${res.filter((r) => !r.ok).length} échec(s)`);
      const r = await api.residusScan();
      setReport(r);
      setSelectedFiles(new Set());
      await loadLots();
    } catch (e) {
      notify(String(e));
    } finally {
      setCleaning(false);
    }
  }, [cleanPaths, notify, loadLots]);

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
      const res = await api.registreClean(chemins, true);
      setRegDeleteResults(res);
      setRegModalStep(3); // Correction 3 : passer à l'étape 3 pour afficher les résultats
      notify(`${res.filter((r) => r.ok).length} clé(s) supprimée(s) · ${res.filter((r) => !r.ok).length} échec(s)`);
      if (report) await runRegScan();
    } catch (e) {
      notify(String(e));
    } finally {
      setRegDeleting(false);
    }
  }, [regKeysToDelete, report, runRegScan, notify]);

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

  const lancerDesinstallation = useCallback(async () => {
    if (!desinstallation) return;
    setDesinstallationBusy(true);
    try {
      const message = await api.logicielDesinstaller(desinstallation.id, true);
      notify(message);
      setSuivi({ id: desinstallation.id, nom: desinstallation.nom });
      setSuiviVerifie(false);
      setSuiviErreur("");
      setDesinstallation(null);
    } catch (e) {
      notify(String(e));
    } finally {
      setDesinstallationBusy(false);
    }
  }, [desinstallation, notify]);

  const chercherApresDesinstallation = useCallback(async () => {
    if (!suivi) return;
    setScanning(true);
    setSuiviErreur("");
    try {
      const r = await api.logicielResidusApres(suivi.id);
      setReport(r);
      setSelectedFiles(new Set());
      setRegKeys([]);
      setSelectedRegKeys(new Set());
      setSuiviVerifie(true);
      setScanError("");
      notify(`${r.residus.length} dossier(s) restant(s) trouvé(s) pour ${suivi.nom}. Choisissez les éléments à mettre en quarantaine.`);
    } catch (e) {
      setSuiviErreur(String(e));
    } finally {
      setScanning(false);
    }
  }, [suivi, notify]);

  // --- Computed ---
  const ghostLogiciels = report?.logiciels.filter((l) => l.etat === "fantome") ?? [];
  const installes = (report?.logiciels.filter((l) => l.etat === "installe") ?? []) as LogicielUsage[];
  const limiteSixMois = new Date();
  limiteSixMois.setMonth(limiteSixMois.getMonth() - 6);
  const anciens = installes.filter((l) => l.derniere_utilisation_estimee && new Date(`${l.derniere_utilisation_estimee}T00:00:00`) < limiteSixMois);
  const inconnus = installes.filter((l) => !l.derniere_utilisation_estimee);
  const installesVisibles = (filtreLogiciels === "anciens" ? anciens : filtreLogiciels === "inconnus" ? inconnus : installes)
    .filter((l) => `${l.nom} ${l.editeur}`.toLocaleLowerCase().includes(filtreNomLogiciel.toLocaleLowerCase()));
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

  // --- Render ---
  return (
    <div className="page rs-page">
      {/* Hero */}
      <div className="hero">
        <div>
          <div className="big">Résidus logiciels</div>
          <div className="muted">
            Recherchez les traces possibles de logiciels désinstallés, même si vous avez oublié leur nom. L'analyse ne supprime rien.
          </div>
          {report && (
            <div className="rs-count">
              {ghostLogicielsAvecResidus.length} ancienne(s) inscription(s) avec résidus · {fmt(report.residus.reduce((t, r) => t + r.octets, 0))}
              {report.decouverte && ` · ${pistes.length} dossier(s) supplémentaire(s) à vérifier`}
            </div>
          )}
        </div>
        <button className="btn primary" disabled={scanning} onClick={() => runScan()}>
          {scanning ? <><span className="pulse" /> Analyse…</> : "Analyser"}
        </button>
      </div>

      {suivi && (
        <section className="card rs-card">
          <header><h3>Désinstallation et nettoyage — {suivi.nom}</h3></header>
          <p className="muted" style={{ padding: "0 14px" }}>
            {suiviVerifie
              ? "L'inscription de ce logiciel a disparu et ses restes ont été recherchés. Examinez les résultats ci-dessous, cochez les dossiers à retirer, puis confirmez leur mise en quarantaine. Le registre se recherche séparément."
              : "Terminez les étapes du désinstalleur officiel, puis vérifiez ici. DiskLens contrôle à nouveau les logiciels enregistrés avant de rechercher les restes. Une annulation ou un redémarrage en attente peut bloquer cette étape."}
          </p>
          {suiviErreur && <p className="bad" role="alert" style={{ padding: "0 14px" }}>{suiviErreur}</p>}
          <div className="row" style={{ padding: "10px 14px" }}>
            <button className="btn primary" disabled={scanning} onClick={chercherApresDesinstallation}>{scanning ? "Vérification…" : "Vérifier et chercher les restes"}</button>
            <button className="btn" disabled={scanning} onClick={() => { setSuivi(null); setSuiviErreur(""); setSuiviVerifie(false); }}>Fermer le suivi</button>
          </div>
          <p className="muted small" style={{ padding: "0 14px" }}>Le suivi reste disponible pendant cette session DiskLens. La quarantaine conserve les données : un déplacement sur le même disque ne libère pas d'espace avant la purge du lot.</p>
        </section>
      )}

      {report?.decouverte && (
        <section className="card rs-card">
          <header><h3>Résidus possibles trouvés automatiquement ({pistes.length})</h3><span className="pill rs-verifier">À vérifier · lecture seule</span></header>
          <p className="muted small" style={{ padding: "0 14px" }}>
            Aucun nom à saisir : recherche dans AppData, ProgramData et les dossiers de programmes, avec recoupement des noms, éditeurs, emplacements et processus actifs.
            L'absence d'inscription ne prouve pas une désinstallation : une application portable ou du Store peut être encore utilisée. Les noms sont déduits des dossiers.
            Ces pistes ne sont pas sélectionnées pour le nettoyage.
          </p>
          {pistes.length > 0 && <label className="row" style={{ padding: "0 14px 10px" }}>Filtrer les dossiers
            <input className="grow" value={filtreDecouverte} onChange={(e) => setFiltreDecouverte(e.target.value)} placeholder="Nom supposé ou chemin…" />
          </label>}
          {pistesVisibles.map((p) => (
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
          <details style={{ padding: "10px 14px" }}>
            <summary>Couverture et limites de la recherche · {report.decouverte.dossiers_examines} dossier(s) candidat(s) examiné(s)</summary>
            <p className="muted small">Recherche limitée aux dossiers immédiats et aux produits millésimés chez les éditeurs installés. Dossiers système et partagés exclus. Liens, jonctions et fichiers cloud ignorés. Aucun parcours de vos documents ou de OneDrive. Les autres profils et installations personnalisées ne sont pas couverts entièrement.</p>
            {report.decouverte.limites.map((limite, i) => <p className="muted small" key={i}>{limite}</p>)}
          </details>
        </section>
      )}

      {report && (
        <section className="card rs-card">
          <header><h3>Désinstaller un logiciel et rechercher ses restes ({installes.length})</h3></header>
          <label className="row" style={{ padding: "10px 14px" }}>Rechercher un logiciel installé
            <input className="grow" value={filtreNomLogiciel} onChange={(e) => setFiltreNomLogiciel(e.target.value)} placeholder="Nom ou éditeur…" />
          </label>
          <div className="row" style={{ padding: "10px 14px", flexWrap: "wrap" }}>
            <button className="btn" aria-pressed={filtreLogiciels === "tous"} onClick={() => setFiltreLogiciels("tous")}>Tous ({installes.length})</button>
            <button className="btn" aria-pressed={filtreLogiciels === "anciens"} onClick={() => setFiltreLogiciels("anciens")}>Dernière trace &gt; 6 mois ({anciens.length})</button>
            <button className="btn" aria-pressed={filtreLogiciels === "inconnus"} onClick={() => setFiltreLogiciels("inconnus")}>Usage inconnu ({inconnus.length})</button>
          </div>
          <p className="muted small" style={{ padding: "0 14px" }}>La dernière utilisation est une estimation issue de UserAssist pour ce profil. Une absence de trace ne signifie pas que le logiciel n'a jamais servi. La date d'installation n'est pas une date d'usage.</p>
          {installesVisibles.length === 0 ? <div className="empty"><p>Aucun logiciel dans ce filtre.</p></div> : installesVisibles.map((l) => (
            <div className="rowitem rs-rowitem" key={l.id}>
              <div className="grow">
                <strong>{l.nom}</strong> <span className="muted small">{l.editeur}{l.version ? ` · v${l.version}` : ""}</span>
                <div className="muted small">Date d'installation déclarée : {l.date_installation ?? "inconnue"} · Dernière trace de lancement : {l.derniere_utilisation_estimee ?? "inconnue"} ({l.confiance_usage ?? "inconnue"})</div>
                {l.origine_oem && <div className="muted small">Origine constructeur possible : {l.origine_oem} ({l.confiance_oem ?? "inconnue"})</div>}
                <div className="muted small" title={l.indice_usage}>{l.indice_usage}</div>
              </div>
              <button className="btn" disabled={!l.desinstalleur || desinstallationBusy || scanning || suivi !== null} onClick={() => setDesinstallation(l)} title={suivi ? "Fermez le suivi en cours avant de désinstaller un autre logiciel" : l.desinstalleur ? "Ouvrir le désinstalleur officiel, puis rechercher les restes" : "Aucun désinstalleur enregistré"}>Désinstaller et nettoyer…</button>
            </div>
          ))}
        </section>
      )}

      {/* Recherche manuelle après désinstallation */}
      <form className="card rs-manual" onSubmit={(e) => { e.preventDefault(); void runScan(true); }}>
        <header><h3>Chercher les restes d'un logiciel déjà désinstallé</h3></header>
        <div className="row rs-manual-row">
          <label className="grow">Nom du logiciel<input value={cherche.nom} placeholder="Ex. : Revit 2024" onChange={(e) => setCherche({ ...cherche, nom: e.target.value })} /></label>
          <label className="grow">Éditeur (facultatif)<input value={cherche.editeur} placeholder="Ex. : Autodesk" onChange={(e) => setCherche({ ...cherche, editeur: e.target.value })} /></label>
          <button className="btn primary" disabled={scanning || cherche.nom.trim().length < 4}>Chercher</button>
        </div>
      </form>

      {/* Liste des logiciels fantômes */}
      {ghostLogicielsAvecResidus.length === 0 ? (
        <div className="empty">
          <h2>{report ? "Aucun résidu lié à une ancienne inscription" : "Lance d'abord une analyse"}</h2>
          <p>{report ? "Aucun dossier associé aux inscriptions de désinstallation fantômes n'a été trouvé. Les pistes automatiques éventuelles sont présentées plus haut." : "Le bouton Analyser recherche aussi les dossiers sans logiciel installé associé, sans demander de nom."}</p>
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

      {/* Registre */}
      <section className="card rs-card rs-reg-section">
        <header>
          <h3>Registre</h3>
          <button className="btn" disabled={regScanning || !ghostLogiciels.length} onClick={runRegScan}>
            {regScanning ? <><span className="pulse" /> Recherche…</> : "Chercher dans le registre (lecture seule)"}
          </button>
        </header>

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
      <div className="rs-action-bar">
        {selectedFilesCount > 0 && (
          <button className="btn primary rs-btn-quarantine" disabled={cleaning} onClick={openCleanModal}>
            Mettre en quarantaine ({selectedFilesCount} dossiers · {fmt(selectedFilesSize)})
          </button>
        )}
        {allowRegDelete && selectedRegCount > 0 && (
          <button className="btn danger rs-btn-regdelete" disabled={regDeleting} onClick={openRegModal}>
            Supprimer {selectedRegCount} clé{selectedRegCount > 1 ? "s" : ""} du registre
          </button>
        )}
      </div>

      {/* Quarantaine - Correction 4 : toujours afficher la section, même sans report */}
      <section className="card rs-card rs-quarantine-section">
        <header><h3>Quarantaine</h3></header>
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
      {desinstallation && (
        <div className="overlay" onClick={() => !desinstallationBusy && setDesinstallation(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Ouvrir le désinstalleur officiel ?</h2>
            <p>Windows lancera le désinstalleur enregistré pour <b>{desinstallation.nom}</b>. Suis ses étapes pour confirmer ou annuler la désinstallation. DiskLens vérifiera à nouveau l'entrée avant son lancement.</p>
            <p>Après la fermeture du désinstalleur, utilisez « Vérifier et chercher les restes ». Les fichiers restants seront présentés avec leur taille ; leur nettoyage demande une sélection et une confirmation séparées.</p>
            <div className="row between">
              <button className="btn" disabled={desinstallationBusy} onClick={() => setDesinstallation(null)}>Annuler</button>
              <button className="btn danger" disabled={desinstallationBusy} onClick={lancerDesinstallation}>{desinstallationBusy ? "Ouverture…" : "Ouvrir le désinstalleur"}</button>
            </div>
          </div>
        </div>
      )}
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
