from pathlib import Path
from dataclasses import dataclass
from typing import Union
import shutil

from pydantic import BaseModel


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
    default_sql_query: str
    default_sql_font: str
    sql_keywords: list[str]
    result_pagination_rows_per_page: str
    save_file_history: str
    max_rows: str
    # colors
    colour_browseButton: str
    colour_sqlEdit: str
    colour_executeButton: str
    colour_resultTable: str
    # dirs
    user_app_settings_dir: Path = Path.home() / ".ParVu"
    recents_file: Path = Path(__file__).parent / "history" / "recents.json"
    settings_file: Path = Path(__file__).parent / "settings" / "settings.json"
    usr_recents_file: Path = user_app_settings_dir / "history" / "recents.json"
    usr_settings_file: Path = user_app_settings_dir / "settings" / "settings.json"
    default_settings_file: Path = Path(__file__).parent / "settings" / "default_settings.json"
    static_dir: Path = Path(__file__).parent / "static"
    


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
    def load_settings(cls):
        user_app_settings_dir: Path = Path.home() / ".ParVu"
        # app settings dir doesnt exist - mb first start
        if not user_app_settings_dir.exists():
            # copy defaults from executable folder to user dir
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
                

        # read from user dir
        settings = (user_app_settings_dir / "settings" / "settings.json")
        with settings.open('r') as f:
            settings_data = f.read()

        model = cls.model_validate_json(settings_data)
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






