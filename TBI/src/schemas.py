from dataclasses import dataclass
from pathlib import Path
import re
from pydantic import BaseModel


class Settings(BaseModel):
    default_data_var_name: str
    default_limit: int
    default_sql_font_size: int
    default_sql_query: str
    default_sql_font: str
    sql_keywords: list[str]
    result_pagination_rows_per_page: int
    save_file_history: bool
    settings_file: Path = Path(__file__).parent / "settings.json"

    def process(self):
        self.sql_keywords = list(set([i.upper().strip() for i in self.sql_keywords]))
        if self.default_data_var_name.upper() in self.sql_keywords:
            self.default_data_var_name = self.default_data_var_name.strip() + "_"
        else:
            self.sql_keywords.append(self.default_data_var_name)

        # Substitute $(default_limit) in default_sql_query
        self.default_sql_query = self.default_sql_query.replace("$(default_data_var_name)", str(self.default_data_var_name))
        self.default_sql_query = self.default_sql_query.replace("$(default_limit)", str(self.default_limit))

    @classmethod
    def load_settings(cls):
        # Load settings from JSON file
        json_file = Path(__file__).parent / "settings.json"
        with json_file.open("r") as f:
            settings_data = f.read()
        model = cls.model_validate_json(settings_data)
        model.process()
        return model

    def save_settings(self):
        # Save current settings to JSON file
        settings_json = self.model_dump_json()
        with self.settings_file.open("w") as f:
            f.write(settings_json)
         
settings = Settings.load_settings()