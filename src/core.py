""" module contains core data logic for app """

from pathlib import Path
from typing import Optional, Union, List, Callable

import pandas as pd
import duckdb
import pyarrow.parquet as pq
import pyarrow as pa

from utils import read_table, nth_from_generator, copy_count_gen_items



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
        # origin file
        self.duckdf = self.__read_into_duckdf()
        # for quering
        self.duckdf_query = self.duckdf
        self.columns_query = self.duckdf_query.columns
        self.columns = self.duckdf.columns


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


    def validate(self):
        assert self.path.exists() and \
                self.path.is_file() and \
                self.path.suffix.lower() in [".parquet", ".csv", ".json"],\
                      "Path must be a valid Parquet, CSV or JSON file"

    def get_generator(self, chunksize: int) -> List[pa.RecordBatch]:
        """ returns a list of pyarrow batches """
        return self.duckdf_query.to_arrow_table().to_batches(chunksize)


    def get_nth_batch(self, n: int, chunksize: int, as_df: bool=True):
        pa_batch = nth_from_generator(self.duckdf_query.to_arrow_table().to_batches(chunksize), n)
        return pa_batch.to_pandas() if as_df else pa_batch


    def search(self, 
               search_query: str, 
               column: str, 
               as_df: bool=False,
               case: bool=False)->Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        """ 
        search query string inside column 
            Parameters:
                - query: query string
                - column: column name
                - as_df: return as pandas dataframe
                - case: case sensitive
        """
        # https://duckdb.org/docs/sql/expressions/cast
        like = "LIKE" if case else "ILIKE"
        sql_query = f"""
                    SELECT * 
                    FROM {self.virtual_table_name}
                    WHERE CAST({column} AS VARCHAR) {like} '%{search_query}%'
                    """
        
        duck_res = self.duckdf.query(virtual_table_name=self.virtual_table_name,
                                    sql_query=sql_query)
            
        return duck_res.to_df() if as_df else duck_res
    
    
    def query(self, query: str, as_df: bool=True) -> Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        """ run provided sql query with class lvl setted virtual_table_name name """
        print(f"q: '{query}'")
        print(f"virtual_table_name: {self.virtual_table_name}")
        duck_res = self.duckdf.query(virtual_table_name=self.virtual_table_name, 
                                    sql_query=query)
        # update duckdf_query
        self.duckdf_query = duck_res
        return duck_res.to_pandas() if as_df else duck_res


    def agg_get_uniques(self, column_name: str) -> List[str]:
        """ get unique values for given column """
        # assert column_name in self.columns
        return self.duckdf_query.unique(column_name).to_df()[column_name].to_list()
    
    def __str__(self):
        return f"<ParVuDataReader:{self.path.as_posix()}[{self.columns}]>"
    
    def __repr__(self):
        return self.__str__()



class Data:
    def __init__(self, 
                 path: Path,
                 virtual_table_name: str):
        """ 
        Parameters:
            - path: Path to read data from.
            - virtual_table_name: Name of virtual table, will be used in sql queries
            - query: query string
        """
        self.path = Path(path)
        self.virtual_table_name = virtual_table_name
        self.reader = Reader(path=self.path, virtual_table_name=self.virtual_table_name)
        self.ftype = 'pq' if self.path.suffix == '.parquet' else 'txt'
        # for big data bunch counting can be slow, so do if manually called by `calc_n_batches`
        self.total_batches = "???"
        self.columns = self.reader.columns.copy()


    def get_nth_batch(self, 
                      n: int, 
                      chunksize: int=None, 
                      as_df: bool=True) -> Union[pd.DataFrame, pa.RecordBatch]:
        """  """
        return self.reader.get_nth_batch(n, chunksize, as_df)


    def get_generator(self, chunksize: int) -> List[pa.RecordBatch]:
        return self.reader.get_generator(chunksize)
    

    def get_uniques(self, column_name: str) -> List[str]:
        """get unique values for given column"""
        return self.reader.agg_get_uniques(column_name)
    

    def execute_query(self, query: str, as_df: bool=False, max_chunksize: int=1000) -> Union[List[pa.RecordBatch], pd.DataFrame]:
        """ executes provided query and update duckdf_query """
        return self.reader.query(query, as_df).to_arrow_table().to_batches(max_chunksize=max_chunksize)
    

    def search(self, query: str, column: str, as_df: bool=True, case: bool=False) -> Union[duckdb.DuckDBPyRelation, pd.DataFrame]:
        return self.reader.search(query, column, as_df, case)
    

    def calc_n_batches(self, chunksize: int) -> int:
        """ calculates the number of batches in data. Use carefuly, bc it can take a time for big data """
        gen, n = copy_count_gen_items(self.get_generator(chunksize))
        return n
    

    def reset_duckdb(self):
        """ reset query result table to file table """
        self.reader.duckdf_query = self.reader.duckdf


    def __str__(self):
        return f"<ParVuDataInstance:{self.path.as_posix()}[{self.reader.columns}]>"
    
    def __repr__(self):
        return self.__str__()


