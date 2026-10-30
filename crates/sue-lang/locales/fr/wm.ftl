## L'interface de Sue: menus, Hub, barre d'état et widgets partagés.

## Menu Sue
hub = Hub
studios = Ateliers
tools = Outils
new = Nouveau
new-file-kind = Nouveau { $category } : { $label }
menu-file = Fichier
menu-project = Projet
menu-send-to = Envoyer vers
open-file = Ouvrir un fichier
open-file-ellipsis = Ouvrir un fichier...
open-project = Ouvrir un projet
open-project-ellipsis = Ouvrir un projet...
save-project = Enregistrer le projet
save-project-as = Enregistrer le projet sous
save-project-as-ellipsis = Enregistrer le projet sous...
open-at-launch = Ouvrir { $name } au démarrage
command-palette = Palette de commandes ({ $command_palette })
check-for-updates = Rechercher des mises à jour
settings = Paramètres
quit-sue = Quitter Sue
language-menu = Langue
language-system = Système ({ $language })
language-follow-sue = Suivre Sue ({ $language })
language-changed = Sue parle { $language }
could-not-change-language = Impossible de changer de langue : { $error }
layout-menu = Disposition
layout-automatic = Automatique ({ $layout })
layout-desktop = Bureau
layout-tablet = Tablette
layout-phone = Téléphone
layout-phone-shaped = Format téléphone
layout-changed = Sue utilise la disposition { $layout }
could-not-change-layout = Impossible de changer de disposition : { $error }

## Hub
sue-hub-every-studio-tool-files = Sue : le Hub, les ateliers, les outils, les fichiers, les projets et les paramètres
pinned-sue-opens-here-launch-click = Épinglé : Sue s'ouvre ici au démarrage. Cliquez pour retirer l'épingle
pin-sue-opens-here-launch = Épingler : Sue s'ouvrira ici au démarrage
sue-opens-hub-launch = Sue s'ouvre sur le Hub au démarrage
sue-opens-in-at-launch = Sue s'ouvre dans { $name } au démarrage
could-not-pin-it = Impossible d'épingler : { $error }

## Palette de commandes
type-command-mode-entity-action = Saisissez une commande, un mode ou une action
nothing-matches = Aucun résultat
go-to-the-hub = Aller au Hub
open = Ouvrir { $name }
pin-open-it-at-launch = Épingler { $name } : l'ouvrir au démarrage
unpin-open-hub-launch = Désépingler : ouvrir le Hub au démarrage
open-settings = Ouvrir les paramètres
undo = Annuler
redo = Rétablir

## Barre d'état
there-no-studio-tool-called = Aucun atelier ou outil nommé { $id }
that-is-gone = Cet élément n'existe plus
is-not-available = { $command } n'est pas disponible
nothing-to-redo = Rien à rétablir
nothing-to-undo = Rien à annuler
undo-failed = Échec de l'annulation : { $error }
entities = { $count ->
    [one] { $count } entité
   *[other] { $count } entités
}
undo-depth = Annuler { $depth }
fps = { $fps } FPS
idle = Inactif

## Fenêtres et widgets
close-esc = Fermer (Échap)
files = Fichiers
pick-a-color = Choisir une couleur
none = Aucun
curve-pen-pressure = Pression du stylet
curve-output = Sortie

## Listes du Hub
open-recent = Ouvrir les fichiers récents
recent = Récent
files-projects-you-open-save-show = Les fichiers et projets ouverts ou enregistrés apparaissent ici.
saved-projects = Projets enregistrés
projects-you-save-sue-menu-project = Les projets enregistrés (menu Sue, Projet, Enregistrer le projet) apparaissent ici.
unsaved-work = Travail non enregistré
work-sue-keeps-you-before-saved = Sue conserve ici le travail non enregistré, à récupérer ou supprimer.
saved-2 = Enregistré
opened = Ouvert
kept-newer-than = Conservé le { $value }, plus récent que le { $value2 }
kept-never-saved = Conservé le { $value }, jamais enregistré
really-discard = Supprimer définitivement
discard = Supprimer
recover = Récupérer
ago-just-now = à l'instant
ago-yesterday = hier
ago-minutes = { $count ->
    [one] il y a une minute
   *[other] il y a { $count } minutes
}
ago-hours = { $count ->
    [one] il y a une heure
   *[other] il y a { $count } heures
}
ago-days = { $count ->
    [one] il y a un jour
   *[other] il y a { $count } jours
}
untitled = Sans titre
label-work = Travail
transfer-import = Importer
transfer-export = Exporter
transfer-copy = Copier
 transfer-copy = Copier
click-to-show-it = {"."} Cliquez pour afficher
in-folder = { $lead } dans { $folder }
saved = { $name } enregistré{ $note }{ $folder }{ $click }
saved-in-folder = {" "}dans { $folder }

