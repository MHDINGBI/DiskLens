# DiskLens

Application Windows locale de diagnostic du stockage, de nettoyage choisi et de santé du PC. Interface française, React et Tauri 2 ; analyses déterministes en Rust.

## Lancer l'application

Télécharger la version Windows 64 bits depuis les **[Releases GitHub](https://github.com/MHDINGBI/DiskLens/releases/latest)** :

- **`DiskLens_0.1.0_x64-setup.exe`** : installateur Windows pour la version initiale ; le nom contient la version de chaque livraison.
- **`DiskLens-Windows-x64.exe`** : exécutable portable, sans installation ; nécessite WebView2.
- **`SHA256SUMS.txt`** : empreintes de contrôle des deux exécutables.

Les exécutables ne sont pas signés numériquement : Windows peut afficher un avertissement SmartScreen. Vérifier la provenance du téléchargement et l'empreinte avant exécution. Cette version initiale reste expérimentale ; sauvegarder ses données avant tout nettoyage.

Pour une livraison locale de développement, placer le portable dans `Livraison/DiskLens.exe`, puis double-cliquer sur **Lancer-DiskLens.cmd**. Le lanceur utilise le cache release en repli si nécessaire.

## Dépôt et distributions

Le dépôt contient les sources, les icônes et les fichiers de dépendances verrouillées, mais **aucun build, installateur, cache de compilation ni diagnostic du poste**. Les distributions sont exclusivement attachées aux Releases. L'import GitHub démarre avec un historique propre ; l'ancien historique contenant les diagnostics reste local.

Le workflow **Windows CI et releases** vérifie les pull requests et les lancements manuels. Un tag `vX.Y.Z`, correspondant aux versions du projet, déclenche les tests, la compilation Windows et la publication automatique d'une release avec ses exécutables et leurs empreintes. Voir [le fonctionnement des releases](docs/RELEASES.md).

## Fonctions

- Explorateur de disque, treemap, filtres, Top 20 et historique de croissance.
- Caches et anciens installateurs à examiner ; doublons BLAKE3 avec filtre photos. L'âge d'un installateur ne prouve pas que son logiciel est installé.
- Panier avec vérification des protections, corbeille par défaut et confirmation de suppression. La corbeille libère l'espace seulement après son vidage.
- Inventaire des logiciels avec recherche par nom/éditeur, dernière trace UserAssist estimée et indices OEM prudents. « Désinstaller et nettoyer… » lance le désinstalleur officiel après confirmation. Une fois ses étapes terminées, « Vérifier et chercher les restes » refuse de poursuivre tant que l'inscription du produit subsiste, puis montre les dossiers à sélectionner pour la quarantaine. Le suivi conserve le nom dans la session ; une absence de trace d'usage ne prouve pas l'absence d'utilisation.
- Résidus de logiciels et registre, quarantaine avec manifeste. « Analyser » découvre aussi des dossiers sans logiciel associé, sans saisir de nom : nom supposé, chemin, taille logique, indices et mesures partielles. Ces pistes incertaines restent en lecture seule et hors du panier de nettoyage. Recoupement des installations et processus ; exclusion des emplacements système/partagés, jonctions et fichiers cloud ; recherche bornée dans les emplacements usuels. La désinstallation n'est pas prouvée par une absence d'inscription. Les anciens lots sans marqueur valide sont conservés et leur purge est refusée.
- Processus CPU/RAM, démarrage réversible, tâches de session et services tiers éligibles. Les services passent en mode manuel sans être arrêtés, et leur mode automatique initial est mémorisé pour réactivation. Les composants système et de sécurité restent protégés.
- Repérage de connexions IPv4 et d'extensions Chromium à examiner, état Defender/pare-feu/mises à jour/disques et sauvegardes. Ce repérage ne remplace pas l'antivirus ; Firefox et IPv6 restent hors de ce diagnostic.
- OneDrive : libération locale explicite avec nouvelle mesure après action.
- Rapport mensuel local sans IA : instantané, comparaison, alertes d'espace, historique JSON et exports Markdown/HTML. Le stockage réutilise les derniers scans et signale les données absentes ou anciennes.

Dans **Rapport**, cliquer sur **Générer maintenant**. La planification est désactivée par défaut. Son activation explicite crée `DiskLens-RapportMensuel`, le premier de chaque mois à 09:00 dans la session Windows ouverte ; elle se désactive depuis le même écran. Aucun e-mail automatique. Les exports sont dans `rapports_mensuels` sous le dossier de données de DiskLens (`%APPDATA%\com.omd.disklens`).

Les explications IA facultatives utilisent le fournisseur choisi dans Réglages. Aucun fournisseur IA n'est appelé par les analyses, la CLI ou le rapport mensuel. Les travaux de développement de cette livraison ont utilisé exclusivement des modèles OpenAI.

Le nettoyage des restes demande une sélection explicite et une confirmation. La quarantaine conserve les données : sur le même volume, elle ne libère pas d'espace avant la purge du lot. Un logiciel désinstallé peut laisser des réglages utiles pour une réinstallation. Les composants partagés restent protégés et les autres versions du même produit sont distinguées par année. Les logiciels sans désinstalleur enregistré ou dont la commande n'est pas prise en charge se gèrent depuis les réglages Windows.

## Ligne de commande

```powershell
.\Livraison\DiskLens.exe --help
.\Livraison\DiskLens.exe --scan 'C:\Mon dossier' --json
.\Livraison\DiskLens.exe --rapport-mensuel
.\Livraison\DiskLens.exe --rapport-mensuel --data-dir 'C:\Mes rapports DiskLens'
```

Ces commandes ne créent aucune fenêtre et ne nettoient rien. Un échec retourne un code non nul ; une erreur de rapport mensuel est aussi conservée dans `rapport_mensuel_erreur.log` dans le dossier de données indiqué.

## Développer et vérifier

Prérequis : Node.js, Rust stable, outils de compilation C++ Windows et WebView2. Dépendances JavaScript : `npm ci`. Développement : `npm run tauri dev`.

```powershell
npm run build
cd src-tauri
cargo test --offline
cargo build --release --offline --features tauri/custom-protocol
```

La feature `tauri/custom-protocol` intègre les fichiers de `dist` au binaire distribué. Le dossier `_cache_build` est le cache de compilation configuré. Après une nouvelle compilation, copier `_cache_build/release/disklens.exe` vers `Livraison/DiskLens.exe` pour actualiser la livraison. Pour produire un installateur, utiliser `npm run tauri build` avec les outils NSIS et l'accès réseau éventuellement nécessaires.

Les tests automatiques courants sont hermétiques. Les tests qui modifient le vrai registre ou parcourent la machine sont marqués `ignore` et ne font pas partie de la validation courante. Ne pas lancer les tests ignorés globalement sur une machine de travail.

Les plans de travail et diagnostics du poste restent locaux et ne font pas partie du dépôt public.
