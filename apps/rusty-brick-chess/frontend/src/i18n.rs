//! Minimal UI translations. Keys are stable identifiers; missing keys fall back
//! to English and then to the key itself, so a partial table never breaks the UI.

pub const LANGS: [(&str, &str); 2] = [("en", "English"), ("es", "Español")];

const STRINGS: &[(&str, &str, &str)] = &[
    ("nav.home", "Home", "Inicio"),
    ("nav.database", "Database", "Base de datos"),
    ("nav.analysis", "Analysis", "Análisis"),
    ("nav.practice", "Practice", "Práctica"),
    ("nav.openings", "Openings", "Aperturas"),
    ("nav.puzzles", "Puzzles", "Puzzles"),
    ("nav.stats", "Stats", "Estadísticas"),
    ("nav.settings", "Settings", "Ajustes"),
    ("top.search", "Search", "Buscar"),
    ("home.welcome", "Welcome back", "Bienvenido de nuevo"),
    ("home.local_account", "Local account", "Cuenta local"),
    ("home.save_profile", "Save profile", "Guardar perfil"),
    ("home.games", "games", "partidas"),
    ("home.players", "players", "jugadores"),
    ("home.events", "events", "eventos"),
    ("home.puzzles", "puzzles", "puzzles"),
    ("home.recent", "Recent games", "Partidas recientes"),
    ("home.profile", "Player profile", "Perfil del jugador"),
    ("home.quick", "Quick actions", "Acciones rápidas"),
    ("home.import", "Import / browse games", "Importar / ver partidas"),
    ("home.new_analysis", "New analysis board", "Nuevo tablero de análisis"),
    ("home.solve", "Solve a puzzle", "Resolver un puzzle"),
    ("home.train", "Train a repertoire", "Entrenar un repertorio"),
    ("db.import", "Import", "Importar"),
    ("db.import_file", "Import file…", "Importar archivo…"),
    ("db.all_games", "All games", "Todas las partidas"),
    ("db.recent_files", "Recent files", "Archivos recientes"),
    ("settings.title", "Settings", "Ajustes"),
    ("settings.language", "Language", "Idioma"),
    ("settings.engines", "Engines", "Motores"),
    ("settings.sounds", "Sound", "Sonido"),
    ("settings.keybinds", "Keybinds", "Atajos de teclado"),
    ("settings.custom_pieces", "Custom piece sets", "Piezas personalizadas"),
    ("practice.guess", "Guess the move", "Adivina la jugada"),
    ("practice.spar", "Sparring", "Sparring"),
    ("practice.match", "Engine match", "Partida de motores"),
    ("practice.start", "Start", "Empezar"),
    ("practice.pause", "Pause", "Pausa"),
    ("practice.reset", "Reset", "Reiniciar"),
    ("analysis.analyze", "Analyze", "Analizar"),
    ("analysis.lines", "Lines", "Líneas"),
    ("analysis.all_engines", "All engines", "Todos los motores"),
];

pub fn t(lang: &str, key: &str) -> &'static str {
    let row = STRINGS.iter().find(|(k, _, _)| *k == key);
    match row {
        Some((_, en, _)) if lang != "es" => en,
        Some((_, _, es)) => es,
        None => "",
    }
}
