import re
from typing import Union

from schemas import settings, BadQueryException



class Revisor:
    def __init__(self, query: str):
        """ params:
                query: str - the query to be revised 
        """
        self.query = self.clear_q(query)
    

    def clear_q(self, query: str) -> str:
        """ clear the query from extra whitespaces """
        result = re.sub(r'\s+', ' ', query)
        return result.strip().lower()
    
    def _rule_limit_range(self) -> str:
        """ limit value must be between 0 and settings.max_rows """
        if 'limit' in self.query:
            limit = re.findall(r'limit\s+([0-9]+)', self.query)
            if not limit:
                return BadQueryException(name="No LIMIT", 
                                        message='The query must contain a limit.')
            
            if not limit[0].isdigit():
                return BadQueryException(name='Parse Error', 
                                        message=f"Unable to parse LIMIT valie: '{limit}'")
            
            limit = int(limit[0])
            if not (0 <= limit <= int(settings.max_rows)):
                fixed_query = self.query.replace(f'limit {limit}', f'LIMIT {settings.max_rows}')
                return BadQueryException(name='Bad Limit Range',
                                        message=f"The LIMIT value must be between 0 and {settings.max_rows}.",
                                        result=fixed_query)

    def _rule_no_joins(self) -> str:
        """ no join'ly queries are allowed """
        if {'left', 'right', 'full', 'inner', 'join'}.intersection(set(self.query.split())):
            return BadQueryException(name="Joins",
                                    message="Joins are not allowed in the query - app is multi-tabled yet...")

    def run(self) -> Union[True, BadQueryException]:
        """ Run all rules """
        rules = [self._rule_limit_range, self._rule_no_joins, ]
        for rule in rules:
            rule_res = rule()
            if isinstance(rule_res, BadQueryException):
                return rule_res

        return True
    


    
if __name__ == "__main__":

    query = """  select * \nfrom     data limit  10000  """
    print(Revisor(query).run())
