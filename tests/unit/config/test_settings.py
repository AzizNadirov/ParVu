"""
Unit tests for settings.
"""
from parvu.config.settings import Settings


def test_settings_render_vars():
    settings = Settings(
        default_data_var_name="mydata",
        default_limit=500,
        default_sql_font_size=14,
        default_sql_query="SELECT * FROM $(default_data_var_name) LIMIT $(default_limit)",
        default_sql_font="Monaco",
    )

    result = settings.render_vars("SELECT * FROM $(default_data_var_name)")
    assert result == "SELECT * FROM mydata"

    result = settings.render_vars("LIMIT $(default_limit)")
    assert result == "LIMIT 500"


def test_settings_defaults():
    settings = Settings()
    assert settings.current_theme == "ParVu Light"
    assert settings.current_language == "en"
    assert settings.enable_large_dataset_warning is True


def test_load_without_bundled_resources(tmp_path, monkeypatch):
    """A broken bundle (no resources/settings) must not crash first launch."""
    from parvu.config import settings as settings_mod

    user_dir = tmp_path / "home" / ".ParVu"
    missing = tmp_path / "missing"
    monkeypatch.setattr(settings_mod, "get_legacy_app_dir", lambda: user_dir)
    monkeypatch.setattr(settings_mod, "resolve_settings_path", lambda: missing / "settings")
    monkeypatch.setattr(settings_mod, "resolve_history_path", lambda: missing / "history")

    loaded = settings_mod.SettingsManager.load()

    assert loaded.default_data_var_name == "data"
    assert (user_dir / "settings" / "settings.json").is_file()
    assert (user_dir / "history" / "recents.json").read_text() == '{"recents": []}'
