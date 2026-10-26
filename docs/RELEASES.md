# Releases DiskLens

## Télécharger et utiliser

Ouvrir <https://github.com/MHDINGBI/DiskLens/releases/latest>.

- L'installateur `DiskLens_<version>_x64-setup.exe` installe l'application Windows 64 bits.
- Le portable `DiskLens-Windows-x64.exe` fonctionne sans installation si Microsoft Edge WebView2 est disponible.
- `SHA256SUMS.txt` contient les empreintes des deux exécutables. Une empreinte confirme l'intégrité du téléchargement, pas l'absence de défaut du logiciel.

Les exécutables ne sont pas signés. Windows peut afficher un avertissement SmartScreen. Ne pas désactiver globalement la protection Windows. La version initiale est expérimentale ; conserver une sauvegarde avant tout nettoyage. La compilation et les tests CLI ne remplacent pas une validation visuelle complète de l'interface.

## Préparer la prochaine version

Il suffit de demander à Hermes : **« Prépare et publie la prochaine release DiskLens »**, en précisant les changements et le numéro de version souhaité. Hermes doit présenter la publication exacte et obtenir confirmation avant de pousser le tag.

Procédure technique :

1. Mettre la même version dans `package.json`, `package-lock.json` (projet racine), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (paquet `disklens`) et `src-tauri/tauri.conf.json`.
2. Vérifier les changements ; aucun diagnostic du PC, secret ou build ne doit être ajouté à Git.
3. Valider et envoyer les sources sur `main`.
4. Après confirmation de publication, créer et pousser le tag `vX.Y.Z`, correspondant exactement à la version des trois manifestes.
5. Suivre **Actions → Windows CI et releases**. Le workflow exécute TypeScript/Vite, les tests Rust hermétiques, la compilation Tauri/NSIS et deux contrôles CLI sur un dossier temporaire.
6. Le workflow joint le portable, l'installateur et les empreintes à une release en brouillon. Il vérifie la présence et la taille des trois fichiers avant de publier.
7. Vérifier la release publiée et les empreintes des fichiers téléchargés avant d'annoncer la livraison.

Les tags de préversion, par exemple `v0.2.0-beta.1`, doivent correspondre à la version des manifestes et créent une release marquée comme préversion. Ne jamais déplacer un tag publié pour remplacer ses fichiers ; utiliser une nouvelle version.

## Vérifier sans publier

Dans **Actions → Windows CI et releases → Run workflow**, choisir `main`. Ce lancement compile et teste, conserve les distributions comme artefact pendant sept jours et **ne publie pas de release**. Les pull requests vers `main` suivent les mêmes vérifications. Seul un tag déclenche la publication.

## Séparation sources / builds

Le dépôt Git conserve uniquement les sources, les icônes et les verrous de dépendances. `dist`, `node_modules`, `_cache_build`, `target`, `Livraison`, les exécutables, les installateurs, les diagnostics et les secrets sont exclus.

Les fichiers de distribution existent uniquement dans les artefacts Actions et les pièces jointes des Releases. Les anciens diagnostics, scripts propres au poste et l'historique Git qui les contient restent locaux, hors de la branche publique `main`.

La branche locale `master` conserve cet ancien historique : **ne pas la pousser et ne pas utiliser `git push --all` ni `git push --mirror`**. Les envois ordinaires vers `origin` sont configurés pour la seule branche `main`.

Les actions GitHub sont épinglées par empreinte de commit. Les tâches de compilation ont des droits de lecture seuls ; seule la tâche de publication dispose de `contents: write`. Aucun certificat de signature ni jeton personnel n'est nécessaire : la publication utilise le jeton temporaire fourni par GitHub Actions.
