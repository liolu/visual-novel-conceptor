# VN Conceptor — prototype

Un logiciel de création de **Visual Novel** en **Rust + egui**.

- 📝 Éditeur de scénario : scènes, dialogues, choix, variables, conditions, décors, personnages, musique, sons, vidéos.
- ▶ **Test en direct** : lance le jeu dans l'éditeur (panneau ou fenêtre séparée) et **modifie tout pendant qu'il tourne**
  (texte, ordre des nœuds, images, scripts, variables). L'aperçu se met à jour immédiatement,
  et les fichiers modifiés dans un autre logiciel (Photoshop, Audacity…) sont rechargés automatiquement.
- 🧩 **Bouton « Extensions »** dans la barre du haut : ouvre une page pour ajouter des extensions en un clic
  (catalogue intégré), importer un fichier `.vnext`, en installer une depuis une URL (GitHub…) ou en écrire une.
- 🏗 **Compiler** : crée le jeu final, **non modifiable par les joueurs** (pack verrouillé + somme de contrôle, lecteur sans aucun outil d'édition).
- ⬆ **Mise à jour automatique** depuis les *releases* GitHub du dépôt.

## Structure

| Crate | Rôle |
|---|---|
| `crates/vn-core` | Modèle de données, moteur d'exécution, scripts Rhai, format compilé `.vnpak`. Sans interface graphique. |
| `crates/vn-engine` | Affichage egui du jeu : images, animations, son, vidéo. Partagé par l'éditeur et le jeu. |
| `crates/vn-editor` | Le logiciel de création (`vn-editor`). |
| `crates/vn-player` | Le lecteur du jeu compilé (`vn-player`) : PC, Android, Web. |

## Lancer

```bash
cargo run --release -p vn-editor            # le logiciel
cargo run --release -p vn-editor -- chemin/vers/projet
```

Sous Linux, il faut `libasound2-dev` et `pkg-config` pour compiler.

Un projet est un dossier : `project.json` + `assets/` (les images/sons/vidéos) + `build/` (jeu compilé).
Par défaut : `~/VN-Conceptor/mon_visual_novel`. L'enregistrement est automatique.

## Formats pris en charge

- **Images** (Rust pur) : PNG, APNG animé, JPEG, GIF animé, WebP animé, BMP, TIFF, TGA, ICO, DDS, HDR, EXR, QOI, PNM, farbfeld.
  Avec ffmpeg en plus : AVIF, HEIC, JPEG‑XL, etc. (converti automatiquement).
- **Son** (Rust pur) : MP3, OGG/Vorbis, FLAC, WAV, AAC, M4A/ALAC, AIFF, CAF, MKA. Avec ffmpeg : Opus, WMA…
- **Vidéo** : tous les formats lus par **ffmpeg** (MP4, WebM, MKV, AVI, MOV, WMV, FLV, MPEG, OGV…), avec le son.
  Placez `ffmpeg` (ou `ffmpeg.exe`) à côté du logiciel ; il est copié avec le jeu compilé.
  Sans ffmpeg, la vidéo est ignorée avec un message.

## Extensions

Une extension = un petit script [Rhai](https://rhai.rs) + une liste de commandes. Les commandes deviennent
des nœuds dans le menu « ➕ Ajouter un nœud ». Elles sont enregistrées **dans le projet** et donc livrées avec le jeu.

```rhai
fn gagner(args) {
    let v = get_var("argent");
    if type_of(v) != "i64" { v = 0; }
    set_var("argent", v + parse_int(args.montant));
    notify("💰 +" + args.montant);
}
```

Fonctions disponibles : `get_var`, `set_var`, `shake`, `flash`, `notify`, `play_sound`, `jump`, `log`, `random`.
Exemple de fichier à importer : [`extensions/porte_monnaie.vnext`](extensions/porte_monnaie.vnext)
(ou installez-le via l'URL GitHub du fichier).

Dans les dialogues, `{argent}` affiche la valeur d'une variable.

## Compiler le jeu

Bouton **🏗 Compiler** → `projet/build/<nom_du_jeu>/` :

- `<nom_du_jeu>` / `<nom_du_jeu>.exe` : le jeu en **un seul fichier** (le lecteur + le pack), si `vn-player` est à côté de l'éditeur
  (c'est le cas dans les versions publiées et après `cargo build`) ;
- `game.vnpak` : le pack seul (brouillé + somme de contrôle : un pack modifié est refusé).

**Android** (APK) et **Web** (fonctionne aussi sur iPhone via le navigateur) — le pack est intégré au moment de la compilation :

```bash
# Android (nécessite le NDK + cargo install cargo-apk)
VN_EMBED_PACK=/chemin/game.vnpak cargo apk build -p vn-player --lib --release

# Web (nécessite cargo install trunk)
VN_EMBED_PACK=/chemin/game.vnpak trunk build --release crates/vn-player/web/index.html
```

## Plateformes

| | Windows | macOS | Linux | Android | Web / iPhone |
|---|---|---|---|---|---|
| Jeu (`vn-player`) | ✅ | ✅ | ✅ | ✅ (APK Android 8+, sans vidéo) | ✅ (sans vidéo) |
| Logiciel (`vn-editor`) | ✅ | ✅ | ✅ | ⚠ expérimental (pas de sélecteur de fichiers) | ✖ |

## Mise à jour automatique

Au démarrage (et dans ⚙ Paramètres), l'éditeur consulte
`https://api.github.com/repos/liolu/visual-novel-conceptor/releases/latest`.
Si la version est plus récente, il télécharge `vn-editor-<plateforme>` et `vn-player-<plateforme>` puis se remplace lui-même.

Publier une version :

```bash
# 1. changer "version" dans Cargo.toml (ex: 0.2.0)
git commit -am "v0.2.0" && git tag v0.2.0 && git push && git push --tags
```

Le workflow `.github/workflows/release.yml` compile pour Windows, Linux et macOS et publie les fichiers.
Si le dépôt est **privé**, l'API GitHub demande un jeton : définir la variable d'environnement `VN_GITHUB_TOKEN`.

## Limites connues du prototype

- Vidéo sur Android/Web non gérée (pas de ffmpeg).
- Pas encore de sauvegarde/chargement de partie pour le joueur, ni de transitions animées entre décors.
- Les GIF/WebP animés très longs sont entièrement décodés en mémoire.
