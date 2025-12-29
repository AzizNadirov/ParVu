from pathlib import Path
from dataclasses import dataclass
from typing import Union
import shutil

from pydantic import BaseModel
from loguru import logger


@dataclass
class BadQueryException:
    """ if result is query means that query is fixed and just give warning """
    name: str
    message: str
    result: str = None


class Settings(BaseModel):
    """
    Params:
        default_data_var_name: str - name for read dataframe in query
        default_limit: Union[int, str] - default limit for sql query
        default_sql_font_size: Union[int, str]
        default_sql_query: str
        default_sql_font: str
        sql_keywords: list[str]
        result_pagination_rows_per_page: str
        save_file_history: str
        max_rows: str - max rows for limit in sql query
    """
    # data
    default_data_var_name: str
    default_limit: Union[int, str]
    default_sql_font_size: Union[int, str]
    default_result_font_size: Union[int, str]
    default_sql_query: str
    default_sql_font: str
    sql_keywords: list[str]
    result_pagination_rows_per_page: str
    save_file_history: str
    max_rows: str
    # theme
    current_theme: str = "ParVu Light"
    # language
    current_language: str = "en"  # ISO 639-1 code (en, ru, az)
    # large dataset warning settings
    enable_large_dataset_warning: bool = True
    warning_criteria: str = "rows"  # "rows", "cells", or "filesize"
    warning_threshold_rows: int = 1000000
    warning_threshold_cells: int = 10000000
    warning_threshold_filesize_mb: int = 100
    # colors (legacy, kept for backward compatibility)
    colour_browseButton: str = "#E8F5E9"
    colour_sqlEdit: str = "#FFF9C4"
    colour_executeButton: str = "#E1F5FE"
    colour_resultTable: str = "#FFFFFF"
    colour_tableInfoButton: str = "#F3E5F5"
    # dirs
    user_app_settings_dir: Path = Path.home() / ".ParVu"
    recents_file: Path = Path(__file__).parent / "history" / "recents.json"
    settings_file: Path = Path(__file__).parent / "settings" / "settings.json"
    usr_recents_file: Path = user_app_settings_dir / "history" / "recents.json"
    usr_settings_file: Path = user_app_settings_dir / "settings" / "settings.json"
    default_settings_file: Path = Path(__file__).parent / "settings" / "default_settings.json"
    static_dir: Path = Path(__file__).parent / "static"
    user_logs_dir: Path = user_app_settings_dir / "logs"
    

    def process(self):
        self.sql_keywords = list(set([i.upper().strip() for i in self.sql_keywords]))
        self.recents_file = Path(self.recents_file).resolve()
        self.settings_file = Path(self.settings_file).resolve()
        self.default_settings_file = Path(self.default_settings_file).resolve()
        self.static_dir = Path(self.static_dir).resolve()
    

    def render_vars(self, query: str) -> str:
        """ render inside the query the vars of the settings """
        if not isinstance(query, str):
            return query
        
        query = query.replace("$(default_data_var_name)", str(self.default_data_var_name))
        query = query.replace("$(default_limit)", str(self.default_limit))
        query = query.replace("$(default_sql_font_size)", str(self.default_sql_font_size))
        query = query.replace("$(default_sql_query)", str(self.default_sql_query))
        query = query.replace("$(default_sql_font)", str(self.default_sql_font))
        return query
    
    
    @classmethod
    def reset_user_settings(cls):
        """  """
        user_app_settings_dir: Path = Path.home() / ".ParVu"
        shutil.copytree(Path(__file__).parent / "settings", 
                            user_app_settings_dir / "settings",
                            dirs_exist_ok=True)
            
        shutil.copytree(Path(__file__).parent / "history",
                        user_app_settings_dir / "history",
                        dirs_exist_ok=True)
        
        # fill with default settings
        with (Path(__file__).parent / "settings" / "default_settings.json").open('r') as f:
            with open(Path(__file__).parent / "history" / "recents.json") as r:
                (user_app_settings_dir / 'settings' / 'settings.json').write_text(f.read())
                (user_app_settings_dir / 'history' / 'recents.json').write_text(r.read())

    @classmethod
    def get_user_settings(cls):
        user_app_settings_dir: Path = Path.home() / ".ParVu"
        settings = (user_app_settings_dir / "settings" / "settings.json")
        with settings.open('r') as f:
            settings_data = f.read()

            return cls.model_validate_json(settings_data)


    @classmethod
    def load_settings(cls):
        user_app_settings_dir: Path = Path.home() / ".ParVu"
        # app settings dir doesnt exist - mb first start
        if not user_app_settings_dir.exists():
            cls.reset_user_settings()
        try:
            # read from user dir
            model = cls.get_user_settings()
            model.process()

        except Exception as e:
            # reset and load 
            logger.error(e)
            logger.critical(f"Resetting user settings")
            cls.reset_user_settings()
            model = cls.get_user_settings()
            model.process()

        return model

    def save_settings(self):
        # Save current settings to JSON file
        settings_json = self.model_dump_json()
        # settings_file = self.usr_settings_file.as_posix()
        with open(self.usr_settings_file, "w") as f:
            f.writelines(settings_json.splitlines())

settings = Settings.load_settings()


class Recents(BaseModel):
    """ Recent opened files history """
    recents: list[str]

    @classmethod
    def load_recents(cls):
        # Load recents from JSON file
        with open(settings.usr_recents_file, "r") as f:
            recents_data = f.read()

        model = cls.model_validate_json(recents_data)
        return model
    
    def add_recent(self, path):
        # add browsed file to recents
        self.recents.insert(0, path)
        self.recents = list(set(self.recents))
        self.save_recents()

    def save_recents(self):
        # Save current recents to JSON file
        recents_json = self.model_dump_json()
        with open(settings.usr_recents_file, "w") as f:
            f.writelines(recents_json.splitlines())

recents = Recents.load_recents()






