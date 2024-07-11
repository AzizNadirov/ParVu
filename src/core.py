from pathlib import Path
from typing import Optional, Union, List

import pandas as pd
import duckdb
import pyarrow.parquet as pq
import pyarrow as pa

from utils import read_table, nth_from_generator



class Reader:
    """
        Read data from a file.
    """
    def __init__(self, path: Path, 
                 virtual_table_name: str):
        """
        Parameters:
            - path: Path to read data from.
            - virtual_table_name: Name of virtual table, will be used in sql queries
        """
        self.path = path
        self.virtual_table_name = virtual_table_name
        self.validate()
        self.duckdf = self.__read_into_duckdf()
        self.columns = self._agg_get_columns()

    def __read_into_duckdf(self) -> duckdb.DuckDBPyRelation:
        path_str = str(self.path)
        if self.path.suffix.lower() == '.parquet':
            return duckdb.read_parquet(path_str)
        elif self.path.suffix.lower() == '.csv':
            return duckdb.read_csv(path_str)
        elif self.path.suffix.lower() == '.json':
            return duckdb.read_json(path_str)
        else:
            raise ValueError(f"File extension {self.path.suffix} is not supported")
        

    def read_data(self): ...

    def validate(self):
        assert self.path.exists() and \
                self.path.is_file() and \
                self.path.suffix.lower() in [".parquet", ".csv", ".json"],\
                      "Path must be a valid Parquet, CSV or JSON file"

    def get_generator(self, chunksize: int): ...

    def get_nth_batch(self, n: int, chunksize: int, as_df: bool=True): ...

    def search(self, 
               query: str, 
               column: str, 
               as_df: bool=True,
               case: bool=False)->Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        """ 
        search query string inside column 
            Parameters:
                - query: query string
                - column: column name
                - as_df: return as pandas dataframe
                - case: case sensitive
        """
        like = "LIKE" if case else "ILIKE"
        sql_query = f"""
                    SELECT * 
                    FROM {self.virtual_table_name}
                    WHERE {column} {like} '%{query}%'
                    """
        duck_res = self.duckdf.query(virtual_table_name=self.virtual_table_name,
                                     sql_query=sql_query)
        return duck_res.to_pandas() if as_df else duck_res
    
    
    def query(self, query: str, as_df: bool=True) -> Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        """ run provided sql query with class lvl setted virtual_table_name name """
        duck_res = self.duckdf.query(virtual_table_name=self.virtual_table_name, 
                                 query=query)
        return duck_res.to_pandas() if as_df else duck_res
    


    def _agg_get_columns(self)->List[str]:
        """ returns columns as list """
        return self.duckdf.columns


    def agg_get_uniques(self, column_name: str) -> List[str]:
        """ get unique values for given column """
        assert column_name in self.columns
        return self.duckdf.unique(column_name).to_df()[column_name].to_list()



class ParquetReader(Reader):
    """
        Read Parquet data from a file.
    """
    def __init__(self, 
                 path: Path,
                 virtual_table_name: str):
        super().__init__(path, virtual_table_name)
        self.validate()
        self.arrow = self.read_data()


    def validate(self):
        assert  self.path.exists() and \
                self.path.is_file() and \
                self.path.suffix == ".parquet", "Path must be a valid Parquet file"


    def read_data(self, **kwargs):
        """ read parquet file to pyarrow """
        return pq.ParquetFile(self.path, **kwargs)
    
    
    def get_generator(self, chunksize: int) -> pa.RecordBatch:
        """ returns a generator of pyarrow batches """
        return self.arrow.iter_batches(batch_size=chunksize)
    
    
    def get_nth_batch(self, n: int, chunksize: int, as_df: bool=True) -> Union[pd.DataFrame, pa.RecordBatch]:
        """ returns nth pyarrow batch """
        batch = nth_from_generator(self.get_generator(chunksize), n)
        return batch.to_pandas() if as_df else batch
    
    

class PDTextReader(Reader):
    """ Base class for Pandas based json, csv readers """
    def __init__(self, path: Path, virtual_table_name: str):
        super().__init__(path, virtual_table_name)
        


    def validate(self):
        assert  self.path.exists() and \
                self.path.is_file() and \
                self.path.suffix.lower() in [".csv", ".json"], \
                    "Path must be a valid csv or json file"
        
    
    def read_data(self): ...
    

    def get_generator(self, chunksize: int)->'pd.io.parsers.readers.TextFileReader':
        return read_table(self.path, chunksize=chunksize)


    def get_nth_batch(self, n: int, chunksize: int) -> pd.DataFrame:
        return nth_from_generator(self.get_generator(chunksize=chunksize), n)
    


class Data:
    def __init__(self, 
                 path: Path,
                 virtual_table_name: str):
        """  """
        self.path = Path(path)
        self.virtual_table_name = virtual_table_name
        self.reader = self._get_reader()
        self.ftype = 'pq' if self.path.suffix == '.parquet' else 'txt'


    def _get_reader(self) -> Reader:
        """ returns reader according to the file extension """
        if self.path.suffix == ".parquet":
            return ParquetReader(self.path, self.virtual_table_name)
        elif self.path.suffix in (".csv", ".json"):
            return PDTextReader(self.path, self.virtual_table_name)
        else:
            raise ValueError(f"File extension {self.path.suffix} is not supported")
        

    def get_nth_batch(self, n: int, chunksize: int, as_df: bool=True) -> Union[pd.DataFrame, pa.RecordBatch]:
        return self.reader.get_nth_batch(n, chunksize, as_df) if self.ftype == 'pq' \
            else self.reader.get_nth_batch(n, chunksize)


    def get_generator(self, chunksize: int) -> Union[pa.RecordBatch, 'pd.io.parsers.readers.TextFileReader']:
        return self.reader.get_generator(chunksize) if self.ftype == 'pq' \
            else self.reader.get_generator(chunksize)


    def get_columns(self) -> List[str]:
        return self.reader.columns.copy()
    

    def get_uniques(self, column_name: str) -> List[str]:
        return self.reader.agg_get_uniques(column_name)
    

    def query(self, query: str, as_df: bool=True) -> Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        return self.reader.query(query, as_df)
    

    def search(self, query: str, column: str, as_df: bool=True) -> Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        return self.reader.search(query, column, as_df)
    
    def _calc_n_batches(self, chunksize: int) -> int:
        n_batches = 0
        for _ in self.get_generator(chunksize):
            n_batches += 1
        return n_batches



