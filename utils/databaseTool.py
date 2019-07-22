import sqlite3
import datetime

class MrcDatabase(object):
    '''
    MRC SQLite Toolkit
    '''
    def __init__(self, sqlitePath:str):

        self.__conn = sqlite3.connect(sqlitePath)
        

    def insertArticle(self, article:str):
        query = 'INSERT into article VALUES (NULL, ?, ?)'
        timestamp = datetime.datetime.utcnow().replace(tzinfo=datetime.timezone.utc).isoformat()
        with self.__conn.cursor() as cursor:
            cursor.execute(query, (article, timestamp))

    def insertQuestionAnswer(self, question:str, answerStart:str, answerString:str):
        query = 'INSERT into question_answer VALUES (NULL, ?, ?, ?)'
        timestamp = datetime.datetime.utcnow().replace(tzinfo=datetime.timezone.utc).isoformat()
        with self.__conn.cursor() as cursor:
            cursor.execute(query, (question, answerStart, answerString, timestamp))

