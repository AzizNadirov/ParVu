""" module contains general purpose tools """
from pathlib import Path
from itertools import islice
from typing import Generator

import pandas as pd



def read_table(file_path: Path, **kwargs) -> pd.DataFrame:
    """ read table from passed file. Supported formats: parquet, csv, json, excel """
    readers = {
        '.parquet': pd.read_parquet,
        '.csv': pd.read_csv,
        '.json': pd.read_json,
        'xlsx': pd.read_excel,
        'xls': pd.read_excel
        }

    ext = file_path.suffix
    if ext not in readers:
        raise ValueError(f"File extension {ext} is not supported")

    return readers[ext](file_path, **kwargs)



def nth_from_generator(generator: Generator, n: int):
    """ Get directly nth item from generator """
    return next(islice(generator, n, n+1), None)