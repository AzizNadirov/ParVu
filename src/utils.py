""" module contains general purpose tools """
from pathlib import Path
from itertools import islice, tee
from typing import Generator, Tuple

import pandas as pd
from loguru import logger


def read_table(file_path: Path, **kwargs) -> pd.DataFrame:
    """ read table from passed file. Supported formats: parquet, csv, json, excel """
    readers = {
        '.parquet': pd.read_parquet,
        '.csv': pd.read_csv,
        '.json': pd.read_json,
        'xlsx': pd.read_excel,
        'xls': pd.read_excel
        }
    logger.info(f"Reading table from {file_path}")

    ext = file_path.suffix
    if ext not in readers:
        raise ValueError(f"File extension {ext} is not supported")

    return readers[ext](file_path, **kwargs)



def nth_from_generator(generator: Generator, n: int):
    """ Get directly nth item from generator """
    logger.debug(f"Getting {n}th item from generator of len {len(generator)} and type: {type(generator)}")
    return next(islice(generator, n, n+1), None)


def copy_count_gen_items(generator) -> Tuple[Generator, int]:
    """ returns a copy of the generator and items count in it """
    logger.debug(f"Copy-count generator of len {len(generator)} and type: {type(generator)}")
    gen_copy, gen_count = tee(generator)
    count = sum(1 for _ in gen_count)
    logger.debug(f"Count: {count}")
    return gen_copy, count