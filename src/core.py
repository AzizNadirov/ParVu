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
        

    def read_data(self): ...


    def validate(self): ...


    def get_generator(self, chunksize: int): ...


    def get_nth_batch(self, n: int, as_df: bool=True): ...


    def search(self, query: str, as_df: bool=True): ...


    def query(self, query: str, as_df: bool=True): ...



class ParquetReader(Reader):
    """
        Read Parquet data from a file.
    """
    def __init__(self, 
                 path: Path,
                 virtual_table_name: str):
        self.path = path
        self.virtual_table_name = virtual_table_name
        self.validate()
        self.arrow = self.read_data()
        self.duckdf = duckdb.from_arrow(self.arrow)
        self.columns = self._agg_get_columns()


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
    
    
    def get_nth_batch(self, n: int, as_df: bool=True) -> Union[pd.DataFrame, pa.RecordBatch]:
        """ returns nth pyarrow batch """
        batch = nth_from_generator(self.get_generator(), n)
        return batch.to_pandas() if as_df else batch
    

    def search(self, 
               query: str, 
               column: str, 
               as_df: bool=True,
               case: bool=False)->Union[pa.RecordBatch, pd.DataFrame]:
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
    
    
    def query(self, query: str, as_df: bool=True) -> Union[pa.RecordBatch, pd.DataFrame]:
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
    

class PDTextReader:
    """ Base class for Pandas based json, csv readers """
    pass


        
class CSVReader(PDTextReader):
    pass



class JSONReader(PDTextReader):
    pass



