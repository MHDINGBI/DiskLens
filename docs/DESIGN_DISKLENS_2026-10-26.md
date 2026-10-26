# Réorganisation de DiskLens

Objectif : rendre la désinstallation et les tâches courantes trouvables dès l'ouverture et réduire les listes inutiles à parcourir.

Architecture retenue : accueil avec accès directs et état réel des disques ; navigation groupée par Stockage, Applications, Suivi et Outils. Logiciels devient une entrée dédiée, avec inventaire indépendant du scan des résidus, recherche, filtres et pagination. Le parcours de désinstallation mène aux restes du produit choisi. Résidus, registre et quarantaine sont présentés dans des vues distinctes, sans perdre les protections ni les confirmations.

La correction limitée à déplacer la liste des logiciels aurait laissé le menu ambigu et les autres tâches difficiles à trouver. Une refonte complète des moteurs de diagnostic n'est pas nécessaire. La réorganisation concerne la navigation, la présentation et l'accès à l'inventaire existant.

Direction visuelle : utilitaire Windows sobre, grille alignée à gauche, palette ardoise et bleu-vert, police locale Segoe UI Variable, pictogrammes SVG homogènes. Les couleurs, espacements et états sont centralisés en variables primitives, sémantiques et de composants. Les préférences de thème existantes sont conservées. Contrastes, focus clavier, réduction des animations et listes bornées sont prioritaires.

Implémentation : navigation/accueil, inventaire Logiciels autonome, découpage des vues de résidus, puis harmonisation CSS. Vérifier l'accès direct aux logiciels sans scan, la séparation des vues, la recherche et pagination, la conservation du parcours officiel et des confirmations, puis compiler et actualiser les binaires livrés/installés avec sauvegarde.

Compétences consultées : UI/UX Pro Max, Frontend Design, Design System et Web Design Guidelines, déjà présentes localement. Recherche complémentaire sur skills.sh ; aucune installation de compétence supplémentaire nécessaire. Les propositions automatiques de pages marketing ne convenaient pas à cet utilitaire : seule la direction de style et les règles d'interaction pertinentes ont été retenues.

Références consultées : [annuaire Skills](https://www.skills.sh/), [Web Interface Guidelines](https://raw.githubusercontent.com/vercel-labs/web-interface-guidelines/main/command.md). Les compétences locales ont fourni la méthode de navigation, la hiérarchie visuelle et l'architecture de variables ; aucune image, police ou bibliothèque d'icônes distante n'est chargée par la nouvelle interface.

## Résultat et vérification

- Accueil avec quatre tâches explicites et capacités réelles des disques ; accès direct Logiciels dans Applications.
- Inventaire seul via `logiciels_liste`, recherche par nom/éditeur sans accents, filtres d'usage prudents et pages de 30 logiciels. Les détails techniques sont repliés.
- Parcours officiel conservé : confirmation, fenêtre du désinstalleur, vérification puis passage aux restes du produit. Aucune désinstallation réelle exécutée pour tester.
- Fichiers, registre avancé et quarantaine dans trois vues ; recherche manuelle facultative repliée et pistes exploratoires paginées par 30.
- Rapport de résidus unique dans App et file d'exécution partagée entre écrans, y compris nettoyage et actualisation dans une même opération. Une ancienne requête ne doit pas remplacer la vérification suivante ni son inventaire autorisé. Les résultats d'une action sont conservés si son actualisation échoue.
- États d'écran conservés après première visite, liens internes par URL, accès clavier au contenu, noms accessibles pour les icônes, dialogue de désinstallation avec focus contenu et restitution, fermeture du dialogue lors d'un changement de rubrique.
- Palette ardoise/bleu-vert et variables primitives/sémantiques/composants dans `tokens.css`, modes clair/sombre, animations réduites selon la préférence système. Contrastes des boutons, liens, textes secondaires et couleurs de carte vérifiés par calcul. La nouvelle police est locale.

`npm run test:ui` : sept tests réussis. Contrôles du rendu React/HTML, accès direct Logiciels sans prérequis de scan, liens depuis les états vides, pagination/séparation des vues, file de requêtes résidus, contrastes des thèmes et des couleurs de la carte. Les cas d'accès direct, prérequis manquant, chevauchement de requêtes et lisibilité de carte ont été observés en échec avant correction.

`npm run build` : TypeScript/Vite réussis, 49 modules ; JavaScript 240,65 Ko, CSS 27,07 Ko avant compression. `cargo test --offline` : 63 tests réussis, zéro échec, huit tests de machine réelle ignorés.

Une relecture indépendante a trouvé et fait corriger le chevauchement des scans, le clavier capturé par un dialogue masqué et le contraste des messages utilisateur. La relecture suivante n'a pas trouvé de nouvelle régression importante dans ces corrections.

Limite : ces tests de rendu et calculs ne remplacent pas un contrôle visuel ni un test complet d'interaction Windows. La navigation, les dialogues et les actions nécessitant les droits administrateur restent à vérifier manuellement dans les conditions réelles d'utilisation. Aucune validation visuelle complète n'est revendiquée pour cette version.

L'ajout de `esbuild` en dépendance de développement explicite utilise la version déjà présente via Vite pour exécuter les tests de rendu de manière reproductible ; aucun paquet n'est ajouté au binaire pour ces tests.

## Distribution

Les sources sont publiées avec la version 0.2.0. Le workflow Windows contrôle les versions, les tests, la compilation et les fichiers de distribution avant publication. Les exécutables, leurs empreintes et les notes figurent dans les Releases, séparément du dépôt de sources. Voir [la procédure de publication](RELEASES.md).
