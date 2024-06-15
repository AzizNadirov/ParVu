from pathlib import Path
from pydantic import BaseModel
from typing import Optional, Union

class Settings(BaseModel):
    default_data_var_name: str
    default_limit: Union[int, str]
    default_sql_font_size: Union[int, str]
    default_sql_query: str
    default_sql_font: str
    sql_keywords: Union[list[str], str]
    result_pagination_rows_per_page: int
    save_file_history: bool
    settings_file: Path = Path(__file__).parent / "settings.json"
    recents_file: Path = Path(__file__).parent / "history" / "recents.json"

    def process(self):
        self.sql_keywords = list(set([i.upper().strip() for i in self.sql_keywords]))
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
            f.writelines(settings_json.splitlines())

settings = Settings.load_settings()




class Recents(BaseModel):
    recents: list[str]

    @classmethod
    def load_recents(cls):
        # Load recents from JSON file
        with settings.recents_file.open("r") as f:
            recents_data = f.read()

        model = cls.model_validate_json(recents_data)
        return model
    
    def add_recent(self, path):
        self.recents.insert(0, path)
        self.recents = list(set(self.recents))
        self.save_recents()

    def save_recents(self):
        # Save current recents to JSON file
        recents_json = self.model_dump_json()
        with settings.recents_file.open("w") as f:
            f.writelines(recents_json.splitlines())


recents = Recents.load_recents()



