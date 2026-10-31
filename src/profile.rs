//! Fichas completas de personajes, locaciones, mundos y objetos: secciones y campos.
//! Los campos se guardan por clave, así se pueden añadir más sin romper proyectos viejos.

pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    /// Filas de alto del campo de texto.
    pub rows: usize,
}

pub struct Section {
    pub title: &'static str,
    pub fields: &'static [Field],
}

const fn f(key: &'static str, label: &'static str, rows: usize) -> Field {
    Field { key, label, rows }
}

pub const CHARACTER: &[Section] = &[
    Section {
        title: "Información personal",
        fields: &[
            f("real_name", "Nombre real", 1),
            f("birth_date", "Fecha de nacimiento", 1),
            f("birth_place", "Lugar de nacimiento", 1),
            f("ethnicity", "Etnia / raza", 1),
            f("job", "Ocupación / trabajo", 1),
            f("job_rank", "Cargo / rango", 1),
            f("job_satisfaction", "Satisfacción laboral", 1),
            f("income", "Ingresos", 1),
            f("residence", "Zona de residencia / entorno", 1),
            f("home", "Descripción del hogar", 2),
            f("neighborhood", "Barrio", 1),
        ],
    },
    Section {
        title: "Físico",
        fields: &[
            f("height", "Altura", 1),
            f("weight", "Peso", 1),
            f("body", "Cuerpo / complexión", 1),
            f("face_shape", "Forma del rostro", 1),
            f("eye_color", "Color de ojos", 1),
            f("eye_shape", "Forma de los ojos", 1),
            f("hair_color", "Color de pelo", 1),
            f("hair_style", "Peinado", 1),
            f("skin_tone", "Tono de piel", 1),
            f("distinguishing", "Rasgo distintivo", 1),
            f("other_face", "Otros rasgos faciales", 1),
            f("other_appearance", "Otra apariencia física", 2),
            f("posture", "Postura", 1),
            f("dress", "Vestimenta", 2),
            f("accessories", "Accesorios", 1),
            f("health", "Salud", 1),
        ],
    },
    Section {
        title: "Personalidad",
        fields: &[
            f("moral", "Moral", 1),
            f("philosophy", "Filosofía", 2),
            f("confidence", "Nivel de confianza", 1),
            f("self_control", "Autocontrol", 1),
            f("intelligence", "Nivel de inteligencia", 1),
            f("motivation", "Motivación", 2),
            f("habits", "Hábitos", 2),
            f("hobbies", "Pasatiempos", 1),
            f("speech", "Forma de hablar", 2),
            f("skills", "Habilidades", 2),
            f("talent", "Fortaleza / talento", 1),
            f("weakness", "Debilidad", 1),
            f("greatest_fear", "Mayor miedo", 1),
            f("secrets", "Secretos", 2),
            f("incompetence", "Incompetencia", 1),
            f("carries", "Lo que siempre lleva (armas, herramientas)", 1),
        ],
    },
    Section {
        title: "Biografía",
        fields: &[
            f("childhood", "Infancia", 3),
            f("family", "Familia", 3),
            f("best_memories", "Mejores recuerdos", 2),
            f("worst_memories", "Peores recuerdos", 2),
            f("defining_moment", "Momento más definitorio", 2),
            f("past_event", "Evento pasado importante", 2),
            f("best_accomplishment", "Mayor logro", 1),
            f("other_accomplishment", "Otro logro", 1),
            f("worst_moment", "Peor momento", 2),
            f("skills_developed", "Cómo desarrolló sus habilidades", 2),
        ],
    },
    Section {
        title: "Historia",
        fields: &[
            f("first_appearance", "Primera aparición", 1),
            f("plot_involvement", "Participación en la trama", 3),
            f("initial_beliefs", "Creencias iniciales", 2),
            f("changed_beliefs", "Creencias tras el cambio", 2),
            f("what_changes", "Qué lleva al cambio", 2),
            f("conflict", "Conflicto", 2),
            f("short_goal", "Objetivo a corto plazo", 1),
            f("long_goal", "Objetivo a largo plazo", 1),
            f("failure", "Fracaso", 1),
            f("organization", "Organización en la que participa", 1),
        ],
    },
    Section {
        title: "Otros",
        fields: &[
            f("long_description", "Descripción larga", 5),
            f("dreamcast", "Actor o actriz ideal", 1),
            f("notes", "Notas", 4),
        ],
    },
];

pub const LOCATION: &[Section] = &[
    Section {
        title: "Información de fondo",
        fields: &[f("history", "Historia", 3)],
    },
    Section {
        title: "Geografía",
        fields: &[
            f("climate", "Clima", 1),
            f("landmarks", "Hitos / puntos de referencia", 2),
            f("nearby", "Lugares cercanos", 2),
        ],
    },
    Section {
        title: "Sentidos",
        fields: &[
            f("sight", "Vista", 2),
            f("smell", "Olfato", 2),
            f("sound", "Sonido", 2),
            f("taste", "Gusto", 1),
            f("touch", "Tacto", 2),
        ],
    },
    Section {
        title: "Otros",
        fields: &[
            f("long_description", "Descripción larga", 5),
            f("notes", "Notas", 4),
        ],
    },
];

pub const WORLD: &[Section] = &[
    Section {
        title: "Información básica",
        fields: &[
            f("overview", "Descripción del mundo", 4),
            f("history", "Historia del mundo", 4),
            f("mood", "Ambiente", 2),
        ],
    },
    Section {
        title: "Geografía y naturaleza",
        fields: &[
            f("geography", "Geografía", 3),
            f("climate", "Clima", 2),
            f("resources", "Recursos naturales", 2),
            f("flora", "Flora", 2),
            f("fauna", "Fauna", 2),
            f("astronomy", "Astronomía", 2),
        ],
    },
    Section {
        title: "Sociedad y cultura",
        fields: &[
            f("culture", "Cultura", 3),
            f("races", "Razas", 2),
            f("languages", "Idiomas", 2),
            f("religions", "Religiones y creencias", 3),
            f("ethics", "Ética y valores", 2),
            f("education", "Educación", 2),
            f("entertainment", "Entretenimiento", 2),
            f("classes", "Sistema de clases o castas", 2),
            f("communication", "Métodos de comunicación", 2),
        ],
    },
    Section {
        title: "Política",
        fields: &[
            f("government", "Formato de gobierno", 2),
            f("government_history", "Historia del gobierno", 3),
            f("laws", "Leyes y sistema judicial", 3),
            f("perception", "Percepción del gobierno", 2),
            f("propaganda", "Propaganda", 2),
            f("opposition", "Organizaciones antigubernamentales", 2),
            f("foreign", "Relaciones exteriores", 2),
            f("war_current", "Guerra actual", 2),
            f("war_past", "Guerras pasadas", 2),
            f("war_potential", "Guerras potenciales", 2),
        ],
    },
    Section {
        title: "Economía",
        fields: &[
            f("economy", "Economía", 3),
            f("currency", "Moneda", 1),
            f("trade", "Comercio", 2),
            f("industry", "Industria", 2),
            f("business", "Negocios", 2),
            f("travel", "Viajes y transporte", 2),
        ],
    },
    Section {
        title: "Ciencia y tecnología",
        fields: &[
            f("science", "Ciencia", 2),
            f("technology", "Tecnología", 2),
            f("physics", "Física", 2),
            f("biology", "Biología", 2),
            f("tech_effect", "Efecto en la tecnología", 2),
            f("society_effect", "Efecto en la sociedad", 2),
            f("world_effect", "Efecto en el mundo", 2),
        ],
    },
    Section {
        title: "Magia",
        fields: &[
            f("magic", "Magia", 3),
            f("magic_rules", "Reglas de la magia", 3),
            f("magic_types", "Tipos de magia", 2),
            f("magic_users", "Quién puede usarla", 2),
        ],
    },
    Section {
        title: "Otros",
        fields: &[f("notes", "Notas", 4)],
    },
];

pub const OBJECT: &[Section] = &[Section {
    title: "Descripción",
    fields: &[
        f("appearance", "Aspecto", 3),
        f("origin", "Origen", 2),
        f("function", "Función / uso", 2),
        f("owner", "Propietario", 1),
        f("location", "Ubicación actual", 1),
        f("importance", "Importancia en la trama", 3),
        f("notes", "Notas", 4),
    ],
}];

/// Ficha de un elemento de una categoría propia (fauna, facciones, naves…).
pub const ELEMENT: &[Section] = &[
    Section {
        title: "Descripción",
        fields: &[
            f("appearance", "Aspecto", 3),
            f("traits", "Características", 3),
            f("origin", "Origen", 2),
            f("location", "Dónde se lo encuentra", 2),
        ],
    },
    Section {
        title: "Historia",
        fields: &[
            f("importance", "Importancia en la trama", 3),
            f("relations", "Relación con otros elementos", 3),
        ],
    },
    Section {
        title: "Otros",
        fields: &[f("notes", "Notas", 4)],
    },
];

/// Copia las secciones fijas a una plantilla que se puede editar.
pub fn owned(sections: &[Section]) -> Vec<crate::model::TemplateSection> {
    sections
        .iter()
        .map(|s| crate::model::TemplateSection {
            title: s.title.to_string(),
            fields: s
                .fields
                .iter()
                .map(|f| crate::model::TemplateField {
                    key: f.key.to_string(),
                    label: f.label.to_string(),
                    rows: f.rows,
                })
                .collect(),
        })
        .collect()
}
