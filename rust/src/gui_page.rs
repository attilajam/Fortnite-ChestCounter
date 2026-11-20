use std::collections::HashMap;

use crate::lang::LangCtx;
use crate::report::{PENCIL_ICON, REPORT_SCRIPT, STYLE, TREASURE_HOUR_SCRIPT, font_face_css};

/// Every localized UI label used by the interactive desktop page.
pub fn texts(lang: &LangCtx) -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::with_capacity(96);
    let mut put = |k: &'static str, fr: &'static str, en: &'static str, es: &'static str| {
        m.insert(k, lang.t(fr, en, es));
    };

    put("home", "Accueil", "Home", "Inicio");
    put("report", "Rapport", "Report", "Informe");
    put("reportUnavailable", "Le rapport est indisponible.", "The report is unavailable.", "El informe no está disponible.");
    put("overlay", "Overlay", "Overlay", "Overlay");
    put("settings", "Réglages", "Settings", "Ajustes");
    put("help", "Aide", "Help", "Ayuda");
    put("total", "Coffres au total", "Chests in total", "Cofres en total");
    put("totalSub", "ouverts ou détruits par toi", "opened or destroyed by you", "abiertos o destruidos por ti");
    put("byHand", "dont {0} ajoutés à la main", "{0} of them added by hand", "de ellos, {0} añadidos a mano");
    put("session", "Session", "Session", "Sesión");
    put("last", "Dernière partie", "Last match", "Última partida");
    put("squad", "Total d'équipe", "Squad total", "Total del equipo");
    put("squadSub", "toi + {0} par tes coéquipiers", "you + {0} by your teammates", "tú + {0} de tus compañeros");
    put("inaccurate", "imprécis", "inaccurate", "impreciso");
    put(
        "inaccurateWhy",
        "Compté dans TES replays : un coffre ouvert par un coéquipier à plus de 150 m de toi n'y est pas, et rien n'est enregistré après ton départ de la partie. Le vrai total de l'équipe est plus haut.",
        "Counted in YOUR replays: a chest a teammate opens more than 150 m away from you is not in them, and nothing is recorded once you leave the match. The real squad total is higher.",
        "Contado en TUS repeticiones: un cofre que un compañero abre a más de 150 m de ti no está en ellas, y no se graba nada después de que salgas de la partida. El total real del equipo es mayor.",
    );
    put("treasures", "Trésors", "Treasures", "Tesoros");
    put("matches", "parties", "matches", "partidas");
    put("perMatch", "{0} par partie", "{0} per match", "{0} por partida");
    put("openReport", "Ouvrir le rapport", "Open the report", "Abrir el informe");
    put("showOverlay", "Afficher l'overlay", "Show the overlay", "Mostrar el overlay");
    put("hideOverlay", "Masquer l'overlay", "Hide the overlay", "Ocultar el overlay");
    put("lock", "Verrouiller l'overlay", "Lock the overlay", "Bloquear el overlay");
    put("unlock", "Déverrouiller pour le déplacer", "Unlock to move it", "Desbloquear para moverlo");
    put("activity", "Activité", "Activity", "Actividad");
    put("noActivity", "Rien pour l'instant.", "Nothing yet.", "Nada por ahora.");
    put("analyzing", "Analyse de tes replays…", "Analyzing your replays…", "Analizando tus repeticiones…");
    put("whatsNew", "Nouveautés de la {0}", "What's new in {0}", "Novedades de la {0}");
    put("newVersion", "Nouvelle version {0} disponible", "New version {0} available", "Nueva versión {0} disponible");
    put("install", "Installer", "Install", "Instalar");
    put("checkUpdate", "Rechercher une mise à jour", "Check for updates", "Buscar actualizaciones");
    put("welcome", "Bienvenue !", "Welcome!", "¡Bienvenido!");
    put(
        "welcomeText",
        "Active d'abord les rediffusions dans Fortnite : Paramètres → Jeu → Rediffusions → Enregistrer les rediffusions. Ensuite, laisse le compteur ouvert : chaque partie est comptée quelques secondes après sa fin. Des coffres ouverts avant ? Le crayon de « Coffres au total » permet de les ajouter.",
        "First turn on replays in Fortnite: Settings → Game → Replays → Record Replays. Then keep the counter open: each match is counted a few seconds after it ends. Chests opened before? The pencil of \"Chests in total\" adds them.",
        "Primero activa las repeticiones en Fortnite: Ajustes → Juego → Repeticiones → Grabar repeticiones. Luego deja el contador abierto: cada partida se cuenta unos segundos después de terminar. ¿Cofres abiertos antes? El lápiz de «Cofres en total» permite añadirlos.",
    );
    put("ok", "OK", "OK", "OK");
    put("moreInfo", "En savoir plus", "More info", "Más información");
    put("wMin", "Réduire", "Minimize", "Minimizar");
    put("wMax", "Agrandir", "Maximize", "Maximizar");
    put("wRestore", "Niveau inférieur", "Restore down", "Restaurar");
    put("wClose", "Fermer", "Close", "Cerrar");
    put("keepOpen", "Garder le compteur ouvert", "Keep the counter open", "Mantener el contador abierto");
    put(
        "autoCloseText",
        "Fortnite est fermé : le compteur, ouvert avec Fortnite, va se fermer tout seul.",
        "Fortnite is closed: the counter, opened with Fortnite, is about to close by itself.",
        "Fortnite está cerrado: el contador, abierto con Fortnite, se va a cerrar solo.",
    );
    put(
        "welcomeQuick",
        "Tes premiers réglages (tu les retrouves à tout moment dans Réglages) :",
        "Your first settings (you find them at any time in Settings):",
        "Tus primeros ajustes (los encuentras en cualquier momento en Ajustes):",
    );
    put("welcomeDone", "C'est parti !", "Let's go!", "¡Vamos!");
    put("inGame", "Overlay en jeu", "In-game overlay", "Overlay en el juego");
    put(
        "inGameHelp",
        "Petite fenêtre toujours au premier plan qui affiche tes coffres en jeu (Fortnite en « Plein écran fenêtré »). Ctrl+Maj+O : afficher / masquer, Ctrl+Maj+L : verrouiller.",
        "Small always-on-top window that shows your chests in game (Fortnite in \"Windowed Fullscreen\"). Ctrl+Shift+O: show / hide, Ctrl+Shift+L: lock.",
        "Pequeña ventana siempre visible que muestra tus cofres en el juego (Fortnite en «Pantalla completa en ventana»). Ctrl+Mayús+O: mostrar / ocultar, Ctrl+Mayús+L: bloquear.",
    );
    put("style", "Style", "Style", "Estilo");
    put("hud", "HUD (intégré au jeu)", "HUD (blends into the game)", "HUD (integrado en el juego)");
    put("classic", "Classique (carte sombre)", "Classic (dark card)", "Clásico (tarjeta oscura)");
    put("size", "Taille", "Size", "Tamaño");
    put("opacity", "Opacité", "Opacity", "Opacidad");
    put("squadToggle", "Afficher le total d'équipe", "Show the squad total", "Mostrar el total del equipo");
    put(
        "squadHelp",
        "Tes coffres + ceux de tous tes coéquipiers (même aléatoires), sur l'overlay et l'accueil.",
        "Your chests + those of all your teammates (random ones too), on the overlay and the home tab.",
        "Tus cofres + los de todos tus compañeros (también los aleatorios), en el overlay y el inicio.",
    );
    put("obs", "Overlay pour OBS (stream)", "Overlay for OBS (stream)", "Overlay para OBS (directo)");
    put(
        "obsSteps",
        "Dans OBS : Sources → + → Navigateur, coche « Fichier local », choisis ce fichier. Largeur 360, hauteur 220 (450 × 300 en classique). Pour ne l'avoir que sur le stream, désactive l'overlay en jeu : la page continue d'être mise à jour.",
        "In OBS: Sources → + → Browser, tick \"Local file\", pick this file. Width 360, height 220 (450 × 300 for classic). To have it on the stream only, turn the in-game overlay off: the page keeps being updated.",
        "En OBS: Fuentes → + → Navegador, marca «Archivo local», elige este archivo. Ancho 360, alto 220 (450 × 300 en clásico). Para tenerlo solo en el directo, desactiva el overlay en el juego: la página se sigue actualizando.",
    );
    put("copy", "Copier le chemin", "Copy the path", "Copiar la ruta");
    put("copied", "Copié !", "Copied!", "¡Copiado!");
    put("preview", "Aperçu", "Preview", "Vista previa");
    put("language", "Langue", "Language", "Idioma");
    put("times", "Heures affichées", "Times shown in", "Horas mostradas en");
    put("timesHelp", "Dans le rapport et ici : semaines, heures du trésor, parties.", "In the report and here: weeks, Treasure Hours, matches.", "En el informe y aquí: semanas, horas del tesoro, partidas.");
    put("timesLocal", "Heure locale ({0})", "Local time ({0})", "Hora local ({0})");
    put("timesUtc", "UTC (heure du jeu)", "UTC (game time)", "UTC (hora del juego)");
    put("clock", "Horloge", "Clock", "Reloj");
    put("clockAuto", "Comme Windows ({0})", "Same as Windows ({0})", "Como Windows ({0})");
    put("clock12", "12 heures (10:54 PM)", "12-hour (10:54 PM)", "12 horas (10:54 PM)");
    put("clock24", "24 heures (22:54)", "24-hour (22:54)", "24 horas (22:54)");
    put("h12", "12 h", "12-hour", "12 h");
    put("h24", "24 h", "24-hour", "24 h");
    put("tray", "Réduire dans la zone de notification", "Minimize to the notification area", "Minimizar al área de notificación");
    put(
        "trayHelp",
        "Oui : réduire la fenêtre la range à côté de l'horloge, en bas à droite de la barre des tâches (icône du coffre, parfois derrière la petite flèche ^). Non : elle reste dans la barre des tâches comme une fenêtre normale. Dans les deux cas, le compteur continue de compter.",
        "Yes: minimizing the window puts it next to the clock, at the bottom right of the taskbar (chest icon, sometimes behind the small ^ arrow). No: it stays in the taskbar like a normal window. Either way, the counter keeps counting.",
        "Sí: minimizar la ventana la coloca junto al reloj, abajo a la derecha de la barra de tareas (icono del cofre, a veces detrás de la pequeña flecha ^). No: se queda en la barra de tareas como una ventana normal. En ambos casos, el contador sigue contando.",
    );
    put("footerWindow", "La croix de la fenêtre ferme le compteur (après confirmation).", "The close button of the window stops the counter (after asking).", "La cruz de la ventana cierra el contador (tras confirmarlo).");
    put("withFortnite", "Ouvrir avec Fortnite", "Open with Fortnite", "Abrir con Fortnite");
    put("withFortniteHelp", "Un petit processus invisible attend Fortnite et ouvre le compteur, qui se ferme après.", "A tiny invisible process waits for Fortnite and opens the counter, which closes afterwards.", "Un pequeño proceso invisible espera a Fortnite y abre el contador, que se cierra después.");
    put("updates", "Mises à jour", "Updates", "Actualizaciones");
    put("upPrompt", "Proposées", "Offered", "Propuestas");
    put("upAuto", "Installées automatiquement", "Installed automatically", "Instaladas automáticamente");
    put("upOff", "Désactivées", "Off", "Desactivadas");
    put("profile", "Compte (profil)", "Account (profile)", "Cuenta (perfil)");
    put("demos", "Dossier des replays", "Replays folder", "Carpeta de repeticiones");
    put("demosMissing", "Introuvable : Fortnite le crée à la première partie enregistrée.", "Not found: Fortnite creates it with the first recorded match.", "No encontrada: Fortnite la crea con la primera partida grabada.");
    put("save", "Enregistrer", "Save", "Guardar");
    put("editChests", "Modifier mon nombre de coffres", "Edit my number of chests", "Modificar mi número de cofres");
    put("close", "Fermer", "Close", "Cerrar");
    put("manual", "Coffres ajoutés à la main", "Chests added by hand", "Cofres añadidos a mano");
    put(
        "manualHelp",
        "Les coffres ouverts avant d'utiliser le compteur. Chaque nombre s'ajoute à ce que tu as déjà ajouté (un nombre négatif en retire) ; la croix annule un ajout.",
        "The chests opened before using the counter. Each number is added to what you already added (a negative number removes some); the cross undoes an addition.",
        "Los cofres abiertos antes de usar el contador. Cada número se suma a lo que ya añadiste (un número negativo quita cofres); la cruz anula un añadido.",
    );
    put("manualTotal", "Ajoutés à la main : {0}", "Added by hand: {0}", "Añadidos a mano: {0}");
    put("addBtn", "Ajouter", "Add", "Añadir");
    put("removeEntry", "Annuler cet ajout", "Undo this addition", "Anular este añadido");
    put("thisWeek", "Ouverts cette semaine", "Opened this week", "Abiertos esta semana");
    put("thisWeekHelp", "comptent aussi pour les trésors hebdo de la semaine {0}", "also count for the weekly treasures of week {0}", "cuentan también para los tesoros semanales de la semana {0}");
    put("estimate", "Ou estimer à partir de tes parties", "Or estimate from your matches", "O estimar a partir de tus partidas");
    put("estimateHelp", "Nombre de parties Battle Royale de la saison, dans Fortnite : Carrière → Statistiques.", "Number of Battle Royale matches this season, in Fortnite: Career → Stats.", "Número de partidas de Battle Royale de la temporada, en Fortnite: Carrera → Estadísticas.");
    put("career", "Parties jouées", "Matches played", "Partidas jugadas");
    put("compute", "Calculer", "Compute", "Calcular");
    put("add", "Ajouter {0} coffres", "Add {0} chests", "Añadir {0} cofres");
    put("estimateTooFew", "Il faut au moins {0} parties comptées pour une moyenne fiable.", "At least {0} counted matches are needed for a reliable average.", "Hacen falta al menos {0} partidas contadas para una media fiable.");
    put("estimateNothing", "Rien à ajouter : tes parties comptées couvrent déjà ce nombre.", "Nothing to add: your counted matches already cover that number.", "Nada que añadir: tus partidas contadas ya cubren ese número.");
    put("dataFolder", "Ouvrir le dossier des données", "Open the data folder", "Abrir la carpeta de datos");
    put("errorLog", "Ouvrir le journal d'erreurs", "Open the error log", "Abrir el registro de errores");
    put("issue", "Signaler un problème (GitHub)", "Report a problem (GitHub)", "Informar de un problema (GitHub)");
    put("project", "Page du projet (GitHub)", "Project page (GitHub)", "Página del proyecto (GitHub)");
    put(
        "helpText",
        "Le compteur lit les replays que Fortnite enregistre et compte tes coffres quelques secondes après chaque partie (Battle Royale de la saison seulement). Laisse-le ouvert pendant que tu joues : tu peux réduire la fenêtre, il continue de compter.",
        "The counter reads the replays Fortnite saves and counts your chests a few seconds after each match (the season's Battle Royale only). Keep it open while you play: you can minimize the window, it keeps counting.",
        "El contador lee las repeticiones que guarda Fortnite y cuenta tus cofres unos segundos después de cada partida (solo el Battle Royale de la temporada). Déjalo abierto mientras juegas: puedes minimizar la ventana, sigue contando.",
    );
    put("uninstall", "Désinstaller ChestCounter", "Uninstall ChestCounter", "Desinstalar ChestCounter");
    put("keepData", "Garder mon historique et mes réglages", "Keep my history and settings", "Conservar mi historial y mis ajustes");
    put("deleteData", "Tout supprimer", "Delete everything", "Borrarlo todo");
    put("uninstallConfirm", "Désinstaller maintenant ? Le compteur se ferme et ChestCounter.exe est supprimé.", "Uninstall now? The counter closes and ChestCounter.exe is deleted.", "¿Desinstalar ahora? El contador se cierra y se borra ChestCounter.exe.");
    put("quit", "Quitter le compteur", "Quit the counter", "Salir del contador");
    put("quitConfirm", "Arrêter le compteur ? Les parties jouées ensuite ne seront comptées qu'à sa prochaine ouverture.", "Stop the counter? Matches played afterwards are only counted when it is opened again.", "¿Detener el contador? Las partidas jugadas después solo se cuentan cuando se vuelva a abrir.");
    put("closed", "Le compteur est arrêté. Tu peux fermer cette fenêtre.", "The counter is stopped. You can close this window.", "El contador está detenido. Puedes cerrar esta ventana.");
    put("offline", "Le compteur ne répond plus (fermé ?). Relance ChestCounter.exe.", "The counter does not answer (closed?). Start ChestCounter.exe again.", "El contador no responde (¿cerrado?). Vuelve a abrir ChestCounter.exe.");
    put("footer", "Fermer cette fenêtre ne coupe pas le compteur.", "Closing this window does not stop the counter.", "Cerrar esta ventana no detiene el contador.");
    m
}

pub fn html(token: &str) -> String {
    TEMPLATE
        .replace("__TOKEN__", token)
        .replace("__FONT__", font_face_css())
        .replace("__PENCIL__", PENCIL_ICON)
        .replace("__REPORT_STYLE__", &STYLE.replace("@media (max-width:", "@container (max-width:"))
        .replace("__REPORT_SCRIPT__", REPORT_SCRIPT)
        .replace("__TREASURE_HOUR_SCRIPT__", TREASURE_HOUR_SCRIPT)
}

const TEMPLATE: &str = r####"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Chest Counter</title>
<style>__FONT__
:root{
  color-scheme:dark;
  --font:"Segoe UI Variable Text","Segoe UI",Inter,system-ui,-apple-system,sans-serif;
  --bg:#0b0f14;--surface:#121822;--surface-2:#17202d;--surface-3:#1e293b;
  --border:rgba(255,255,255,.08);--border-strong:rgba(255,255,255,.15);
  --ink:#e8edf5;--ink-bright:#ffffff;--muted:#8a9ab5;
  --gold:#eab308;--gold-ink:#facc15;--gold-soft:rgba(234,179,8,.14);
  --teal:#14b8a6;--teal-ink:#2dd4bf;--teal-soft:rgba(20,184,166,.15);
  --warn:#f87171;--warn-bg:rgba(248,113,113,.11);
  --star:#c084fc;--sky:#38bdf8;--track:rgba(255,255,255,.08);
  --shadow:0 1px 2px rgba(0,0,0,.35),0 8px 24px rgba(0,0,0,.22);
  --hero-bg:radial-gradient(125% 115% at 0% 0%,rgba(234,179,8,.17) 0%,#121822 60%);
  --hero-border:rgba(234,179,8,.34);
}
@media (prefers-color-scheme:light){:root{
  color-scheme:light;
  --bg:#f6f5f0;--surface:#ffffff;--surface-2:#f1efe7;--surface-3:#e6e2d5;
  --border:rgba(15,23,42,.09);--border-strong:rgba(15,23,42,.17);
  --ink:#18202c;--ink-bright:#0b0f14;--muted:#5a6577;
  --gold:#b47806;--gold-ink:#926004;--gold-soft:rgba(180,120,6,.11);
  --teal:#0f766e;--teal-ink:#0f766e;--teal-soft:rgba(15,118,110,.11);
  --warn:#dc2626;--warn-bg:rgba(220,38,38,.08);
  --star:#7e22ce;--sky:#0284c7;--track:rgba(15,23,42,.08);
  --shadow:0 1px 2px rgba(15,23,42,.05),0 6px 18px rgba(15,23,42,.05);
  --hero-bg:radial-gradient(125% 115% at 0% 0%,rgba(234,179,8,.16) 0%,#ffffff 64%);
  --hero-border:rgba(180,120,6,.36);
}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.55 var(--font);-webkit-font-smoothing:antialiased}
main{max-width:1080px;margin:0 auto;padding:24px 20px 44px;display:flex;flex-direction:column;gap:20px}
.pirate{font-family:"Pirata One",var(--font),serif;font-weight:400;letter-spacing:.01em}
.num{font-variant-numeric:tabular-nums;font-feature-settings:"tnum" 1;letter-spacing:-.015em}
#app{display:flex;flex-direction:column;gap:20px}
#hdr{display:flex;flex-direction:column;gap:14px}
#view{display:flex;flex-direction:column;gap:18px}
#view:empty{display:none}
#reportPane{min-width:0;container-type:inline-size}
header{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:12px;border-bottom:1px solid var(--border);padding-bottom:14px}
.brand{display:flex;align-items:center;gap:12px}
.brandmark{width:42px;height:42px;border-radius:11px;background:var(--gold-soft);border:1px solid var(--hero-border);display:grid;place-items:center;flex:none}
h1{margin:0;font-size:32px;line-height:1.05;color:var(--ink-bright)}
.who{display:flex;align-items:center;gap:8px;flex-wrap:wrap;color:var(--muted);font-size:12.5px;margin-top:2px}
.who select{min-height:28px;padding:2px 28px 2px 8px;font-size:12.5px;border-radius:7px}
.pill{display:inline-flex;align-items:center;gap:8px;padding:6px 12px;border-radius:999px;background:var(--surface);border:1px solid var(--border);color:var(--ink-bright);font-weight:600;font-size:12.5px;box-shadow:var(--shadow)}
.pill i{width:8px;height:8px;border-radius:50%;background:var(--teal);flex:none}
.pill.live i{background:#ef4444;box-shadow:0 0 0 0 rgba(239,68,68,.55);animation:pulse 1.8s infinite}
.pill.busy i{background:var(--gold)}.pill.warn i{background:var(--warn)}
@keyframes pulse{0%{box-shadow:0 0 0 0 rgba(239,68,68,.55)}70%{box-shadow:0 0 0 7px rgba(239,68,68,0)}100%{box-shadow:0 0 0 0 rgba(239,68,68,0)}}
nav{display:inline-flex;align-self:flex-start;gap:4px;flex-wrap:wrap;padding:4px;border-radius:11px;background:var(--surface);border:1px solid var(--border);box-shadow:var(--shadow)}
nav button{min-height:36px;padding:6px 15px;border-radius:8px;border:1px solid transparent;background:transparent;color:var(--muted);font:600 13.5px var(--font);cursor:pointer;transition:background .13s,color .13s,border-color .13s}
nav button:hover{color:var(--ink-bright);background:var(--surface-2)}
nav button.on{background:var(--gold-soft);color:var(--gold-ink);border-color:var(--hero-border)}
nav button:focus-visible{outline:2px solid var(--gold);outline-offset:2px}
h2{margin:0 0 10px;font-size:16px;font-weight:650;letter-spacing:-.01em;color:var(--ink-bright)}
.tiles{display:grid;grid-template-columns:repeat(auto-fit,minmax(175px,1fr));gap:12px}
.tile{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:12px;padding:15px 17px;box-shadow:var(--shadow);display:flex;flex-direction:column;justify-content:space-between;gap:4px}
.tile .v{font-size:30px;font-weight:700;line-height:1.08;color:var(--ink-bright)}
.tile .l{font-size:12.5px;color:var(--muted);line-height:1.4}
.tile.hero{position:relative;background:var(--hero-bg);border:1px solid var(--hero-border)}
.tile.hero .k{font-size:11px;font-weight:650;letter-spacing:.11em;text-transform:uppercase;color:var(--gold-ink)}
.tile.hero .v{font-size:48px;color:var(--gold-ink);line-height:1.02;letter-spacing:-.025em;margin:2px 0}
.tile.hero .l{color:var(--ink);opacity:.88}
.editbtn{position:absolute;top:11px;right:11px;width:34px;height:34px;padding:0;border-radius:9px;border:1px solid var(--hero-border);background:var(--gold-soft);color:var(--gold-ink);cursor:pointer;display:grid;place-items:center;transition:background .14s}
.editbtn:hover{background:rgba(234,179,8,.24)}.editbtn:focus-visible{outline:2px solid var(--gold);outline-offset:2px}
dialog{width:min(540px,calc(100% - 32px));padding:20px 22px;border:1px solid var(--border-strong);border-radius:14px;background:var(--surface);color:var(--ink);box-shadow:0 24px 60px rgba(0,0,0,.55)}
dialog::backdrop{background:rgba(5,8,13,.68);backdrop-filter:blur(4px)}
.dhead{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:8px}
.dhead h2{margin:0;color:var(--ink-bright);font-size:17px}
.dclose{width:34px;height:34px;border-radius:8px;border:1px solid var(--border);background:var(--surface-2);color:var(--muted);font-size:20px;line-height:1;cursor:pointer;display:grid;place-items:center}
.dclose:hover{color:var(--ink-bright);border-color:var(--border-strong)}
.card{background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:12px;padding:16px 18px;box-shadow:var(--shadow)}
.row{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:13px 0;border-bottom:1px solid var(--border)}
.row:first-child{padding-top:2px}.row:last-child{border-bottom:none;padding-bottom:2px}
.row .t{font-weight:600;color:var(--ink-bright)}.row .h{font-size:12.5px;color:var(--muted);margin-top:2px;line-height:1.45}
.actions{display:flex;flex-wrap:wrap;gap:8px}
button.b{min-height:38px;padding:7px 15px;border-radius:9px;border:1px solid var(--border-strong);background:var(--surface-2);color:var(--ink-bright);font:600 13.5px var(--font);cursor:pointer;transition:background .13s,border-color .13s}
button.b:hover{background:var(--surface-3);border-color:var(--gold)}
button.b:focus-visible{outline:2px solid var(--gold);outline-offset:2px}
button.b.main{background:var(--gold);color:#0b0f14;border-color:var(--gold);font-weight:700}
button.b.main:hover{filter:brightness(1.06)}
button.b.danger{border-color:rgba(248,113,113,.38);background:var(--warn-bg);color:var(--warn)}
button.b.danger:hover{border-color:var(--warn)}
select,input[type=text],input[type=number]{min-height:38px;padding:6px 11px;border-radius:8px;border:1px solid var(--border-strong);background:var(--surface-2);color:var(--ink-bright);font:13.5px var(--font);outline:none;transition:background .13s,border-color .13s}
.select-wrap{position:relative;display:inline-block;max-width:100%;vertical-align:middle}
.select-wrap::after{content:"";position:absolute;right:13px;top:50%;width:7px;height:7px;border:solid var(--muted);border-width:0 2px 2px 0;transform:translateY(-70%) rotate(45deg);pointer-events:none}
select{appearance:none;max-width:100%;padding-right:32px;cursor:pointer}
select:hover{background:var(--surface-3)}
select option{color:var(--ink-bright);background:var(--surface)}
select:focus,input[type=text]:focus,input[type=number]:focus{border-color:var(--gold)}
select:focus-visible{outline:2px solid var(--gold);outline-offset:2px}
input[type=text]{width:min(520px,100%)}
.switch{position:relative;display:inline-block;width:46px;height:26px;flex:none;vertical-align:middle}
.switch input{opacity:0;width:100%;height:100%;margin:0;cursor:pointer;position:absolute;z-index:1}
.switch span{position:absolute;inset:0;border-radius:999px;background:var(--surface-3);border:1px solid var(--border-strong);transition:background .15s,border-color .15s}
.switch span:after{content:"";position:absolute;top:2px;left:2px;width:20px;height:20px;border-radius:50%;background:#fff;transition:transform .15s;box-shadow:0 1px 3px rgba(0,0,0,.3)}
.switch input:checked+span{background:var(--teal);border-color:var(--teal)}.switch input:checked+span:after{transform:translateX(20px)}
.switch input:focus-visible+span{outline:2px solid var(--gold);outline-offset:2px}
.badge{display:inline-block;margin-left:6px;padding:1px 7px;border-radius:999px;background:var(--warn-bg);color:var(--warn);font-size:11px;font-weight:650;vertical-align:1px;cursor:help}
.log{font:12.5px/1.6 "Cascadia Mono",Consolas,ui-monospace,monospace;max-height:260px;overflow:auto;white-space:pre-wrap;background:var(--surface);border-radius:12px}
.log .err{color:var(--warn)}.log .warn{color:#f59e0b}.log .ok{color:var(--teal-ink)}.log .info{color:var(--sky)}.log .tre{color:var(--star)}.log .mut{color:var(--muted)}
.bar{height:6px;background:var(--track);border-radius:999px;overflow:hidden;margin-top:8px}.bar i{display:block;height:100%;background:var(--gold);border-radius:999px}
.path{font:12.5px "Cascadia Mono",Consolas,ui-monospace,monospace;word-break:break-all;background:var(--surface-2);border:1px solid var(--border);color:var(--ink-bright);padding:8px 10px;border-radius:8px;display:block;margin:6px 0}
.note{font-size:13px;color:var(--muted)}
.hidden{display:none!important}
footer{display:flex;justify-content:space-between;align-items:center;gap:12px;flex-wrap:wrap;border-top:1px solid var(--border);padding-top:14px;color:var(--muted);font-size:12.5px}
.estimate{margin-top:8px;font-weight:600;color:var(--ink-bright)}
.mtotal{font-weight:600;margin-bottom:8px;color:var(--ink-bright)}
.mweek{display:flex;align-items:center;gap:8px;margin-top:10px;cursor:pointer;accent-color:var(--teal)}
.mweek b{white-space:nowrap;color:var(--ink-bright)}
.mweek span{color:var(--muted);font-size:12.5px}
.mhist{list-style:none;margin:12px 0 0;padding:0;max-height:190px;overflow:auto}
.mhist li{display:flex;align-items:center;gap:12px;padding:6px 2px;border-top:1px solid var(--border)}
.mhist b{min-width:68px;text-align:right;color:var(--gold-ink)}
.mhist span{flex:1;color:var(--muted);font-size:12.5px}
.mhist .mx{border:0;background:none;color:var(--muted);font-size:18px;line-height:1;cursor:pointer;padding:2px 7px;border-radius:6px}
.mhist .mx:hover{color:var(--warn);background:var(--warn-bg)}
.notice{position:relative;display:flex;gap:12px;align-items:flex-start;border-left:4px solid var(--gold)}
.notice.warning{border-left-color:var(--warn);background:var(--warn-bg);border-color:rgba(248,113,113,.3)}
.notice .ni{flex:none;width:22px;height:22px;border-radius:50%;display:grid;place-items:center;font:700 13px var(--font);background:var(--gold);color:#0b0f14;margin-top:1px}
.notice.warning .ni{background:var(--warn);color:#fff}
.notice .nb{flex:1;min-width:0}.notice .nt{font-weight:650;color:var(--ink-bright);margin-bottom:2px}.notice .nx{white-space:pre-line;font-size:13.5px}
.notice .actions{margin-top:10px}
.notice .nclose{flex:none;width:30px;height:30px;margin:-4px -6px 0 0;border:0;border-radius:7px;background:transparent;color:var(--muted);font-size:19px;line-height:1;cursor:pointer}
.notice .nclose:hover{background:var(--surface-2);color:var(--ink-bright)}
@media (max-width:640px){.row{flex-direction:column;align-items:flex-start}}
.tbar{display:none}
body.windowed{height:100vh;overflow:hidden}
body.windowed .tbar{display:flex;justify-content:flex-end;position:fixed;top:0;left:0;right:0;height:34px;background:var(--bg);z-index:20;app-region:drag;user-select:none}
body.windowed #scroll{position:absolute;top:34px;left:0;right:0;bottom:0;overflow:auto}
body.windowed header{app-region:drag}
body.windowed header select,body.windowed header button{app-region:no-drag}
.wctl{display:flex;app-region:no-drag}
.wctl button{width:46px;height:34px;padding:0;border:0;border-radius:0;background:transparent;color:var(--muted);display:grid;place-items:center;cursor:pointer;transition:background .12s,color .12s}
.wctl button:hover{background:var(--surface-2);color:var(--ink-bright)}
.wctl button.x:hover{background:#dc2626;color:#fff}
.wctl button:focus-visible{outline:2px solid var(--gold);outline-offset:-2px}
.wctl svg{display:block}
#scroll::-webkit-scrollbar,html::-webkit-scrollbar{width:10px}
#scroll::-webkit-scrollbar-track,html::-webkit-scrollbar-track{background:var(--bg)}
#scroll::-webkit-scrollbar-thumb,html::-webkit-scrollbar-thumb{background:var(--surface-3);border-radius:999px;border:2px solid var(--bg)}
#scroll::-webkit-scrollbar-thumb:hover,html::-webkit-scrollbar-thumb:hover{background:var(--muted)}
.log::-webkit-scrollbar{width:8px}
.log::-webkit-scrollbar-track{background:transparent}
.log::-webkit-scrollbar-thumb{background:var(--surface-3);border-radius:999px}
</style></head><body><div class="tbar"><div class="wctl">
  <button type="button" id="wmin"><svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" stroke-width="1.1"/></svg></button>
  <button type="button" id="wmax"><svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect id="wmaxicon" x=".55" y=".55" width="8.9" height="8.9" fill="none" stroke="currentColor" stroke-width="1.1"/><path id="wresicon" d="M2.5 2.5V.55h6.95v6.95H7.5M.55 2.5h6.95v6.95H.55z" fill="none" stroke="currentColor" stroke-width="1.1" style="display:none"/></svg></button>
  <button type="button" id="wclose" class="x"><svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5.5l9 9M9.5.5l-9 9" stroke="currentColor" stroke-width="1.2"/></svg></button>
</div></div><div id="scroll"><main><div id="app"><p class="note">…</p></div><section id="activityPane" class="hidden"><h2></h2><div class="card log"></div></section><section id="reportPane" class="hidden"><div id="reportContent"></div><p id="reportStatus" class="note" role="status" hidden></p></section><div id="foot"></div></main></div>
<dialog id="dlg" aria-labelledby="dlgT"><div class="dhead"><h2 id="dlgT"></h2><button type="button" class="dclose" id="dlgX">×</button></div><div id="dlgB"></div></dialog>
<template id="reportStyle"><style>__REPORT_STYLE__</style></template>
<script>
__REPORT_SCRIPT__
__TREASURE_HOUR_SCRIPT__
const TOKEN='__TOKEN__';let S=null,tab=(()=>{const h=location.hash.slice(1);if(['home','report','overlay','settings','help'].includes(h))return h;try{return localStorage.getItem('cc-gui-tab')||'home'}catch(e){return 'home'}})(),estimate=null,stopped=false,copied=false;
const $=s=>document.querySelector(s);
const reportRoot=$('#reportContent').attachShadow({mode:'open'});
const host=window.chrome&&window.chrome.webview;let maximized=false;
if(host&&new URLSearchParams(location.search).get('w')==='1')document.body.classList.add('windowed');
const scroller=()=>document.body.classList.contains('windowed')?document.getElementById('scroll'):document.scrollingElement;
function winBtn(a){if(host)host.postMessage({win:a})}
function winTitles(){if(!S)return;const T=S.txt;$('#wmin').title=T.wMin;$('#wmin').setAttribute('aria-label',T.wMin);const m=maximized?T.wRestore:T.wMax;$('#wmax').title=m;$('#wmax').setAttribute('aria-label',m);$('#wclose').title=T.wClose;$('#wclose').setAttribute('aria-label',T.wClose)}
if(host)host.addEventListener('message',e=>{const d=e.data;if(d&&typeof d.maximized==='boolean'){maximized=d.maximized;$('#wmaxicon').style.display=maximized?'none':'';$('#wresicon').style.display=maximized?'':'none';winTitles()}});
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmt=(s,...a)=>a.reduce((r,v,i)=>r.replace('{'+i+'}',v),s);
async function api(action,value){const r=await fetch('/api/action',{method:'POST',headers:{'X-Token':TOKEN,'Content-Type':'application/json'},body:JSON.stringify({action,value})});const j=await r.json();await refresh(true);return j.result}
async function refresh(force){if(stopped)return;try{const r=await fetch('/api/state',{headers:{'X-Token':TOKEN}});S=await r.json();render(force)}catch(e){if(S){$('#app').innerHTML='<div class="card">'+esc(S.txt.offline)+'</div>';appHtml=null;$('#activityPane').classList.add('hidden')}}}
function busy(){const a=document.activeElement;return a&&a.closest&&a.closest('#view')&&/INPUT|SELECT|TEXTAREA/.test(a.tagName)}
function busyHeader(){const a=document.activeElement;return a&&a.closest&&a.closest('#hdr')&&a.tagName==='SELECT'}
function sw(id,on,act){return `<label class="switch"><input type="checkbox" id="${id}" ${on?'checked':''} onchange="api('${act}',this.checked)"><span></span></label>`}
function row(title,help,control){return `<div class="row"><div><div class="t">${title}</div>${help?`<div class="h">${help}</div>`:''}</div><div>${control}</div></div>`}
function sel(act,value,opts,label){return `<span class="select-wrap"><select aria-label="${esc(label)}" onchange="api('${act}',this.value)">${opts.map(o=>`<option value="${esc(o[0])}" ${String(o[0])===String(value)?'selected':''}>${esc(o[1])}</option>`).join('')}</select></span>`}
function setTab(t){tab=t;try{localStorage.setItem('cc-gui-tab',t)}catch(e){}render(true)}
function render(force){
  const T=S.txt;document.documentElement.lang=S.lang;
  const head=`<header><div class="brand"><span class="brandmark" aria-hidden="true"><svg width="24" height="24" viewBox="0 0 56 56"><rect x="6" y="21" width="44" height="27" rx="5" fill="#422006" stroke="#eab308" stroke-width="3.2"/><path d="M6 26 Q28 5 50 26" fill="#713f12" stroke="#eab308" stroke-width="3.2"/><rect x="23.5" y="24" width="9" height="10" rx="2.5" fill="#facc15"/><line x1="6" y1="35" x2="50" y2="35" stroke="#eab308" stroke-width="2.2"/></svg></span><div><h1 class="pirate">${esc(S.appName)}</h1><div class="who">${S.profiles.length>1?`<span class="select-wrap"><select aria-label="${esc(T.profile)}" title="${esc(T.profile)}" onchange="api('profile',this.value)">${S.profiles.map(p=>`<option value="${esc(p.key)}" ${p.key===S.profileKey?'selected':''}>${esc(p.name)} (${esc(p.matches)})</option>`).join('')}</select></span>`:`<span>${esc(S.player||'')}</span>`}<span>v${esc(S.version)}</span></div></div></div><span class="pill ${S.status.kind}"><i></i>${esc(S.status.text)}</span></header>
  <nav>${['home','report','overlay','settings','help'].map(t=>`<button class="${t===tab?'on':''}" onclick="setTab('${t}')">${esc(T[t])}</button>`).join('')}</nav>`;
  if(!force&&busyHeader())return;
  if(!force&&busy()){$('#hdr').innerHTML=head;appHtml=null;return}
  if(!force&&pressedAt&&Date.now()-pressedAt<3000)return;
  let v='';
  if(tab==='home')v=home(T);else if(tab==='overlay')v=overlay(T);else if(tab==='settings')v=settings(T);else if(tab==='help')v=help(T);
  const app=`<div id="hdr">${head}</div>${notices(T)}<div id="view">${v}</div>`;
  if(force||app!==appHtml){$('#app').innerHTML=app;appHtml=app}
  const foot=S.settings.windowed?`<footer><span>${esc(T.footerWindow)}</span></footer>`:`<footer><span>${esc(T.footer)}</span><button class="b danger" onclick="quit()">${esc(T.quit)}</button></footer>`;
  if(force||foot!==footHtml){$('#foot').innerHTML=foot;footHtml=foot}
  showActivity(T);showReport();winTitles();
}
let appHtml=null,footHtml=null,pressedAt=0;
document.addEventListener('pointerdown',()=>{pressedAt=Date.now()},true);
['pointerup','pointercancel'].forEach(t=>document.addEventListener(t,()=>{pressedAt=0},true));
let activityHtml=null;
function showActivity(T){
  const pane=$('#activityPane');
  pane.classList.toggle('hidden',tab!=='home');
  if(tab!=='home')return;
  pane.querySelector('h2').textContent=T.activity;
  const log=pane.querySelector('.log');
  const html=S.activity.length?S.activity.map(l=>l.map(s=>`<span class="${s.c}">${esc(s.t)}</span>`).join('')).join('\n'):esc(T.noActivity);
  if(activityHtml===html)return;
  const top=log.scrollTop;
  log.innerHTML=html;
  log.scrollTop=top;
  activityHtml=html;
}
let reportRevision=null,reportLoading=false,stopReportClock=null;
async function showReport(){
  const pane=$('#reportPane'),status=$('#reportStatus');
  pane.classList.toggle('hidden',tab!=='report');
  if(tab!=='report'){
    stopReportClock?.();stopReportClock=null;
    return;
  }
  pane.setAttribute('aria-label',S.txt.report);
  const revision=S.reportRevision;
  if(revision===reportRevision){
    if(!stopReportClock)stopReportClock=startTreasureHour(reportRoot);
    return;
  }
  if(reportLoading)return;
  reportLoading=true;
  try{
    const response=await fetch('/api/report',{headers:{'X-Token':TOKEN}});
    if(!response.ok)throw new Error('Report request failed');
    const html=await response.text();
    if(stopped||tab!=='report'||revision!==S.reportRevision)return;
    const scroll=scroller(),top=scroll.scrollTop;
    const oldSearch=reportRoot.querySelector('.msearch');
    const searchFocused=!!oldSearch&&reportRoot.activeElement===oldSearch;
    const filter=reportRoot.querySelector('.fchip.on')?.dataset.f;
    const template=document.createElement('template');
    template.innerHTML=html;
    const surface=template.content.querySelector('.report-content');
    if(!surface)throw new Error('Report content is missing');
    stopReportClock?.();
    reportRoot.replaceChildren($('#reportStyle').content.cloneNode(true),template.content);
    initReport(reportRoot,surface,(action,value)=>api(action,value).catch(()=>{}));
    reportRoot.getElementById('editchests').onclick=openManual;
    stopReportClock=startTreasureHour(reportRoot);
    const search=reportRoot.querySelector('.msearch');
    if(search&&oldSearch){
      search.value=oldSearch.value;
      search.dispatchEvent(new Event('input'));
      if(searchFocused){
        search.focus({preventScroll:true});
        search.setSelectionRange(oldSearch.selectionStart,oldSearch.selectionEnd);
      }
    }
    if(filter&&filter!=='all')reportRoot.querySelector(`.fchip[data-f="${filter}"]`)?.click();
    scroll.scrollTop=top;
    status.hidden=true;
    reportRevision=revision;
  }catch(e){
    if(reportRevision===null){status.textContent=S.txt.reportUnavailable;status.hidden=false}
  }finally{
    reportLoading=false;
  }
}
function notices(T){return (S.notices||[]).map(n=>`<div class="card notice ${n.level==='warning'?'warning':''}" role="${n.level==='warning'?'alert':'status'}"><span class="ni" aria-hidden="true">${n.level==='warning'?'!':'i'}</span><div class="nb">${n.title?`<div class="nt">${esc(n.title)}</div>`:''}<div class="nx">${esc(n.text)}</div>${n.link?`<div class="actions"><button class="b ${n.level==='warning'?'main':''}" onclick="window.open('${esc(n.link)}','_blank')">${esc(T.moreInfo)}</button></div>`:''}</div>${n.level==='info'?`<button class="nclose" title="${esc(T.close)}" aria-label="${esc(T.close)}" onclick="api('dismissNotice','${esc(n.id)}')">×</button>`:''}</div>`).join('')}
function home(T){
  let h='';
  if(S.autoClose)h+=`<div class="card notice warning"><b>${esc(S.status.text)}</b><br>${esc(T.autoCloseText)}<div class="actions" style="margin-top:10px"><button class="b main" onclick="api('keepOpen')">${esc(T.keepOpen)}</button></div></div>`;
  if(S.firstRun){const s=S.settings;h+=`<div class="card welcome"><h2>${esc(T.welcome)}</h2><p style="margin-top:0">${esc(T.welcomeText)}</p>
    <div class="t" style="font-weight:600;margin-top:14px">${esc(T.welcomeQuick)}</div>
    ${row(esc(T.language),'',sel('language',s.language,s.languages,T.language))}
    ${row(esc(T.inGame),esc(T.inGameHelp),sw('wov',S.overlay.show,'showOverlay'))}
    ${s.windowed?row(esc(T.tray),esc(T.trayHelp),sw('wtr',s.minimizeToTray,'minimizeToTray')):''}
    <div class="actions" style="margin-top:12px"><button class="b main" onclick="api('dismissWelcome')">${esc(T.welcomeDone)}</button></div></div>`}
  if(S.progress)h+=`<div class="card"><b>${esc(T.analyzing)} ${S.progress.done} / ${S.progress.total}</b><div class="bar"><i style="width:${Math.round(100*S.progress.done/Math.max(1,S.progress.total))}%"></i></div></div>`;
  if(S.whatsNew&&S.whatsNew.length)h+=`<div class="card"><h2>${esc(fmt(T.whatsNew,S.version))}</h2><ul>${S.whatsNew.map(n=>`<li>${esc(n)}</li>`).join('')}</ul><button class="b main" onclick="api('dismissWhatsNew')">${esc(T.ok)}</button></div>`;
  if(S.update)h+=`<div class="card"><h2>${esc(fmt(T.newVersion,S.update.version))}</h2><ul>${S.update.notes.map(n=>`<li>${esc(n)}</li>`).join('')}</ul><button class="b main" onclick="api('installUpdate')">${esc(T.install)}</button></div>`;
  h+=`<section class="tiles"><div class="tile hero"><button type="button" class="editbtn" onclick="openManual()" title="${esc(T.editChests)}" aria-label="${esc(T.editChests)}">__PENCIL__</button><div class="k">${esc(T.total)}</div><div class="v num">${esc(S.total)}</div><div class="l">${esc(T.totalSub)}${S.manual?'<br>'+esc(fmt(T.byHand,S.manualText.replace('+',''))):''}</div></div>
    <div class="tile"><div class="v num">${esc(S.session)}</div><div class="l">${esc(T.session)} · ${esc(S.sessionMatches)} ${esc(T.matches)}</div></div>
    <div class="tile"><div class="v num">${esc(S.last?S.last.chests:'—')}</div><div class="l">${esc(T.last)}${S.last?'<br>'+esc(S.last.when)+' · '+esc(S.last.mode):''}</div></div>
    ${S.squad?`<div class="tile"><div class="v num">${esc(S.squad.total)}</div><div class="l">${esc(T.squad)}<span class="badge" title="${esc(T.inaccurateWhy)}">${esc(T.inaccurate)}</span><br>${esc(fmt(T.squadSub,S.squad.team))}</div></div>`:''}
    <div class="tile"><div class="v num" style="color:var(--star)">★ ${esc(S.treasures)}</div><div class="l">${esc(T.treasures)} · ${esc(S.matches)} ${esc(T.matches)} · ${esc(fmt(T.perMatch,S.average))}</div></div></section>`;
  h+=`<div class="actions"><button class="b main" onclick="setTab('report')">${esc(T.openReport)}</button>`;
  if(S.overlay.running)h+=`<button class="b" onclick="api('toggleOverlayVisible')">${esc(S.overlay.visible?T.hideOverlay:T.showOverlay)}</button><button class="b" onclick="api('toggleOverlayLock')">${esc(S.overlay.locked?T.unlock:T.lock)}</button>`;
  if(!S.update&&S.updatesOn)h+=`<button class="b" onclick="api('checkUpdate')">${esc(T.checkUpdate)}</button>`;
  h+='</div>';
  return h;
}
function overlay(T){
  const o=S.overlay;
  let h=`<div class="card">${row(esc(T.inGame),esc(T.inGameHelp),sw('ov',o.show,'showOverlay'))}
    ${row(esc(T.style),'',sel('overlayStyle',o.style,[['hud',T.hud],['classic',T.classic]],T.style))}
    ${o.show?row(esc(T.size),'',sel('overlaySize',o.size,o.sizes.map(x=>[x,x+' %']),T.size))+row(esc(T.opacity),'',sel('overlayOpacity',o.opacity,o.opacities.map(x=>[x,x+' %']),T.opacity)):''}
    ${row(esc(T.squadToggle)+`<span class="badge" title="${esc(T.inaccurateWhy)}">${esc(T.inaccurate)}</span>`,esc(T.squadHelp)+'<br>'+esc(T.inaccurateWhy),sw('sq',o.squad,'showSquadTotal'))}</div>
    <section><h2>${esc(T.obs)}</h2><div class="card"><span class="path">${esc(S.paths.overlay)}</span><p class="note">${esc(T.obsSteps)}</p>
    <div class="actions"><button class="b main" onclick="copyObs()">${esc(copied?T.copied:T.copy)}</button><button class="b" onclick="window.open('/overlay?t='+TOKEN,'_blank')">${esc(T.preview)}</button></div></div></section>`;
  return h;
}
function settings(T){
  const s=S.settings;
  let h=`<div class="card">${row(esc(T.language),'',sel('language',s.language,s.languages,T.language))}
    ${row(esc(T.times),esc(T.timesHelp),sel('timeZone',s.timeZone,[['local',fmt(T.timesLocal,s.localZone)],['utc',T.timesUtc]],T.times))}
    ${row(esc(T.clock),'',sel('clockFormat',s.clockFormat,[['auto',fmt(T.clockAuto,s.windowsClock==='24'?T.h24:T.h12)],['12',T.clock12],['24',T.clock24]],T.clock))}
    ${S.profiles.length>1?row(esc(T.profile),'',sel('profile',S.profileKey,S.profiles.map(p=>[p.key,p.name+' ('+p.matches+' · '+p.chests+')']),T.profile)):''}
    ${s.windowed?row(esc(T.tray),esc(T.trayHelp),sw('tr',s.minimizeToTray,'minimizeToTray')):''}
    ${s.canStartWithFortnite?row(esc(T.withFortnite),esc(T.withFortniteHelp),sw('wf',s.startWithFortnite,'startWithFortnite')):''}
    <div class="row"><div style="flex:1"><div class="t">${esc(T.demos)}</div>${s.demosExists?'':`<div class="h">${esc(T.demosMissing)}</div>`}<div class="actions" style="margin-top:6px"><input type="text" id="demos" value="${esc(s.demos)}"><button class="b" onclick="api('demosFolder',$('#demos').value)">${esc(T.save)}</button></div></div></div></div>`;
  return h;
}
function manualForm(T,p){
  const hist=S.manualHistory||[];
  return `<p class="note" style="margin-top:0">${esc(T.manualHelp)}</p>
    <div class="mtotal">${esc(fmt(T.manualTotal,S.manualText))}</div>
    <div class="actions"><input type="number" id="${p}manual" step="1" placeholder="0" aria-label="${esc(T.manual)}" onkeydown="if(event.key==='Enter')addManual('${p}')"><button class="b main" onclick="addManual('${p}')">${esc(T.addBtn)}</button></div>
    ${S.manualWeek?`<label class="mweek"><input type="checkbox" id="${p}week"><b>${esc(T.thisWeek)}</b><span>${esc(fmt(T.thisWeekHelp,S.manualWeek))}</span></label>`:''}
    ${hist.length?`<ul class="mhist">${hist.map(e=>`<li><b class="num">${esc(e.chests)}</b><span>${esc([e.what,e.when].filter(Boolean).join(' · '))}</span><button type="button" class="mx" title="${esc(T.removeEntry)}" aria-label="${esc(T.removeEntry)}" onclick="removeManual('${p}','${esc(e.id)}')">×</button></li>`).join('')}</ul>`:''}
    <div class="t" style="margin-top:16px;font-weight:600;color:var(--ink-bright)">${esc(T.estimate)}</div><p class="note">${esc(T.estimateHelp)}</p>
    <div class="actions"><input type="number" id="${p}career" min="0" placeholder="${esc(T.career)}" aria-label="${esc(T.career)}"><button class="b" onclick="doEstimate('${p}')">${esc(T.compute)}</button></div>
    <div class="estimate" id="${p}est">${estimate&&estimate.p===p?estimateHtml(T,p):''}</div>`;
}
function estimateHtml(T,p){if(estimate.error)return esc(estimate.error);return esc(estimate.text)+`<div class="actions" style="margin-top:8px"><button class="b main" onclick="addEstimate('${p}')">${esc(fmt(T.add,estimate.chestsText))}</button></div>`}
async function doEstimate(p){const r=await api('estimate',parseInt($('#'+p+'career').value||'0',10));estimate=Object.assign({p},r);const e=$('#'+p+'est');if(e)e.innerHTML=estimateHtml(S.txt,p)}
async function addManual(p){const i=$('#'+p+'manual'),w=$('#'+p+'week'),n=parseInt(i&&i.value||'0',10);if(!n){if(i)i.focus();return}await api('manualAdd',{chests:n,thisWeek:!!(w&&w.checked)});redrawManual(p)}
async function addEstimate(p){if(!estimate||estimate.error)return;await api('manualEstimate',{chests:estimate.chests,career:estimate.career});redrawManual(p)}
async function removeManual(p,id){await api('manualRemove',id);redrawManual(p)}
function redrawManual(p){estimate=null;if(p!=='d'){render(true);return}$('#dlgB').innerHTML=manualForm(S.txt,'d');const i=$('#dmanual');if(i)i.focus()}
function openManual(){const T=S.txt,d=$('#dlg');estimate=null;$('#dlgT').textContent=T.editChests;$('#dlgB').innerHTML=manualForm(T,'d');$('#dlgX').setAttribute('aria-label',T.close);if(!d.open)d.showModal();const i=$('#dmanual');if(i){i.focus();i.select()}}
$('#dlgX').onclick=()=>$('#dlg').close();
$('#wmin').onclick=()=>winBtn('min');$('#wmax').onclick=()=>winBtn('max');$('#wclose').onclick=()=>winBtn('close');
let dlgDownOut=false;
function dlgOutside(e){const r=$('#dlg').getBoundingClientRect();return e.target.id==='dlg'&&(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)}
$('#dlg').addEventListener('pointerdown',e=>{dlgDownOut=dlgOutside(e)});
$('#dlg').addEventListener('click',e=>{if(dlgDownOut&&dlgOutside(e))$('#dlg').close();dlgDownOut=false});
async function copyObs(){await api('copyObsPath');copied=true;render(true);setTimeout(()=>{copied=false;render(true)},2000)}
function help(T){
  return `<div class="card"><p style="margin-top:0">${esc(T.helpText)}</p><div class="actions"><button class="b" onclick="api('openDataFolder')">${esc(T.dataFolder)}</button>${S.paths.errorLog?`<button class="b" onclick="api('openErrorLog')">${esc(T.errorLog)}</button>`:''}<button class="b" onclick="api('openIssues')">${esc(T.issue)}</button><button class="b" onclick="api('openRepo')">${esc(T.project)}</button></div></div>`;
}
function closedView(){stopped=true;stopReportClock?.();$('#app').innerHTML='<div class="card">'+esc(S.txt.closed)+'</div>';$('#foot').innerHTML='';appHtml=footHtml=null;$('#activityPane').classList.add('hidden');$('#reportPane').classList.add('hidden')}
async function quit(){if(!confirm(S.txt.quitConfirm))return;try{await api('quit')}catch(e){}closedView()}
refresh(true);setInterval(()=>refresh(false),2000);
</script></body></html>
"####;
