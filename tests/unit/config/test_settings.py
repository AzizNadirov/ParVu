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
