use oasgen::{OaSchema, Schema, SchemaData, SchemaKind, Type, ObjectType};

/// Helper to create an array schema for a fixed-size array.
fn array_schema<T: OaSchema>() -> Schema {
    Schema::new_array(T::schema())
}

fn make_object(obj: ObjectType) -> Schema {
    Schema {
        data: SchemaData::default(),
        kind: SchemaKind::Type(Type::Object(obj)),
    }
}

fn object_property(obj: &mut ObjectType, name: &str, schema: Schema, required: bool) {
    obj.properties.insert(name.to_string(), schema);
    if required {
        obj.required.push(name.to_string());
    }
}

// --- Manual OaSchema impls for types containing fixed-size arrays ---

impl OaSchema for crate::game_manager::CreateGameResponse {
    fn schema() -> Schema {
        let mut obj = ObjectType::default();
        object_property(&mut obj, "game_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "player_ids", array_schema::<uuid::Uuid>(), true);
        make_object(obj)
    }
}

impl OaSchema for crate::game_manager::GameStateResponse {
    fn schema() -> Schema {
        let mut obj = ObjectType::default();
        object_property(&mut obj, "game_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "short_id", String::schema(), true);
        object_property(&mut obj, "state", crate::State::schema(), true);
        object_property(&mut obj, "team_a_score", <Option<i32>>::schema(), false);
        object_property(&mut obj, "team_b_score", <Option<i32>>::schema(), false);
        object_property(&mut obj, "team_a_bags", <Option<i32>>::schema(), false);
        object_property(&mut obj, "team_b_bags", <Option<i32>>::schema(), false);
        object_property(&mut obj, "current_player_id", <Option<uuid::Uuid>>::schema(), false);
        object_property(&mut obj, "player_names", array_schema::<crate::game_manager::PlayerNameEntry>(), true);
        object_property(&mut obj, "timer_config", <Option<crate::TimerConfig>>::schema(), false);
        object_property(&mut obj, "player_clocks_ms", array_schema::<u64>(), false);
        object_property(&mut obj, "active_player_clock_ms", <Option<u64>>::schema(), false);
        object_property(&mut obj, "table_cards", array_schema::<crate::Card>(), false);
        object_property(&mut obj, "player_bets", array_schema::<i32>(), false);
        object_property(&mut obj, "player_tricks_won", array_schema::<i32>(), false);
        object_property(&mut obj, "last_trick_winner_id", <Option<uuid::Uuid>>::schema(), false);
        make_object(obj)
    }
}

impl OaSchema for crate::matchmaking::MatchResult {
    fn schema() -> Schema {
        let mut obj = ObjectType::default();
        object_property(&mut obj, "game_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "player_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "player_short_id", String::schema(), true);
        object_property(&mut obj, "player_url", String::schema(), true);
        object_property(&mut obj, "player_ids", array_schema::<uuid::Uuid>(), true);
        object_property(&mut obj, "player_names", array_schema::<Option<String>>(), true);
        object_property(&mut obj, "short_id", String::schema(), true);
        make_object(obj)
    }
}

impl OaSchema for crate::challenges::ChallengeStatus {
    fn schema() -> Schema {
        let mut obj = ObjectType::default();
        object_property(&mut obj, "challenge_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "short_id", String::schema(), true);
        object_property(&mut obj, "max_points", i32::schema(), true);
        object_property(&mut obj, "timer_config", <Option<crate::TimerConfig>>::schema(), false);
        object_property(&mut obj, "seats", array_schema::<Option<crate::challenges::SeatInfo>>(), true);
        // Flattened ChallengeStatusKind fields
        object_property(&mut obj, "status", String::schema(), true);
        object_property(&mut obj, "game_id", <Option<uuid::Uuid>>::schema(), false);
        object_property(&mut obj, "expires_at_epoch_secs", u64::schema(), true);
        make_object(obj)
    }
}

impl OaSchema for crate::challenges::ChallengeSummary {
    fn schema() -> Schema {
        let mut obj = ObjectType::default();
        object_property(&mut obj, "challenge_id", uuid::Uuid::schema(), true);
        object_property(&mut obj, "short_id", String::schema(), true);
        object_property(&mut obj, "max_points", i32::schema(), true);
        object_property(&mut obj, "seats_filled", <usize>::schema(), true);
        object_property(&mut obj, "seats", array_schema::<Option<crate::challenges::SeatInfo>>(), true);
        make_object(obj)
    }
}
