# LINE → Signal

Convertit un pack de stickers LINE (fixe ou animé) en pack de stickers Signal.

1. **Import** : colle l'URL du pack (`https://store.line.me/stickershop/product/11537/fr`) ou son ID.
   L'archive vient de `dl.stickershop.line.naver.jp/products/0/0/1/<ID>/iphone/stickers@2x.zip`
   (ou `stickerpack@2x.zip` pour les packs animés).
2. **Sélection** : choisis les stickers, leur emoji et la couverture (clic droit).
3. **Pack Signal** : titre, auteur, puis
   - **Exporter le dossier** : des fichiers prêts pour Signal Desktop → *Fichier → Créer/téléverser un pack de stickers* ;
   - **Envoyer à Signal** : envoi direct, qui renvoie un lien `https://signal.art/addstickers/…`.

## Conversion

Signal demande des images carrées de 512 px au plus, de 300 Kio au plus, et des APNG qui bouclent à l'infini.
Par défaut, les stickers gardent leur taille d'origine, centrés sur un carré transparent. Si un fichier
dépasse 300 Kio, l'application passe en palette 256 couleurs, puis réduit la taille, puis saute des images.
Le détail des réductions s'affiche au survol de la taille de chaque sticker.

## Identifiants Signal (envoi direct)

Dans Signal Desktop : *Affichage → Outils de développement*, onglet *Console* :

```js
window.reduxStore.getState().items.uuid_id
window.reduxStore.getState().items.password
```

Les identifiants peuvent être mémorisés dans le trousseau du système.

## Installation

Les binaires de chaque release ne sont pas signés :

- **macOS** : `xattr -cr "LINE to Signal.app"` avant la première ouverture ;
- **Windows** : SmartScreen → *Informations complémentaires* → *Exécuter quand même* ;
- **Linux** : pour avoir l'icône sous Wayland, copier `line2signal.desktop` dans
  `~/.local/share/applications/` et `line2signal.png` dans `~/.local/share/icons/`,
  avec `line2signal` dans le `PATH`.

## Crédits

Polices [Nunito](https://github.com/googlefonts/nunito), [Noto Sans JP](https://github.com/notofonts/noto-cjk)
et [Noto Color Emoji](https://github.com/googlefonts/noto-emoji) (`assets/fonts`) : SIL Open Font License 1.1.

## Développement

```sh
cargo run
cargo test                    # tests hors ligne
cargo test -- --ignored       # téléchargements réels depuis LINE, captures d'écran
SCREENSHOT_DIR=/tmp/captures cargo test --release screenshots -- --ignored
```

Pousser un tag `v*` construit les binaires Linux, Windows et macOS (universel) et publie la release.
