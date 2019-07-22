import sqlite3
import datetime
import random

class MrcDatabase(object):
    """MRC Database SQLite toolkit.

    Attributes:
        sqlitePath (str): The sqlite file path.

    """
    def __init__(self, sqlitePath:str):

        self.__timezone = datetime.timezone(datetime.timedelta(hours=8))
        self.__conn = sqlite3.connect(sqlitePath)
        self.__maxArticleNumber = self.__getMaxArticleNumber()
        self.__maxQuestionAndAnswerNumber = self.__getMaxQuestionAndAnswerNumber()

    def insertArticle(self, article:str):
        query = 'INSERT into article VALUES (NULL, ?, ?)'
        timestamp = datetime.datetime.now().replace(tzinfo=self.__timezone).isoformat()
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query, (article, timestamp))

    def insertQuestionAnswer(self, articleId:int, question:str, answerStart:int, answerString:str):
        query = 'INSERT into question_answer VALUES (NULL, ?, ?, ?, ?, ?)'
        timestamp = datetime.datetime.now().replace(tzinfo=self.__timezone).isoformat()
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query, (articleId, question, answerStart, answerString, timestamp))

    def commit(self):
        self.__conn.commit()

    def __getMaxArticleNumber(self):
        query = 'SELECT count(*) FROM article'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query)
            result = cursor.fetchone()[0]
        return result

    def __getMaxQuestionAndAnswerNumber(self):
        query = 'SELECT count(*) FROM question_answer'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query)
            result = cursor.fetchone()[0]
        return result


    def getRandomArticle(self) -> str:
        idx = random.randint(1, self.__maxArticleNumber)
        query = 'SELECT context FROM article WHERE id = ?'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query,(idx,))
            result = cursor.fetchone()[0]
        return result


    def getArticleById(self, idx:int) -> str:
        if idx > self.__maxArticleNumber:
            result = 'ERROR: <ArticleOutOfBound>'
        else:
            query = 'SELECT context FROM article WHERE id = ?'
            with self.__conn:
                cursor = self.__conn.cursor()
                cursor.execute(query,(idx,))
                result = cursor.fetchone()[0]
        return result

    def getAllArticle(self):
        query = 'SELECT context FROM article'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query)
            result = cursor.fetchall()
        return result


    def getQuestionAndAnswerByArticleId(self, articleId:int) -> (str, int, str):
        if articleId > self.__maxArticleNumber:
            result = ('NULL', -1, 'NULL')
        else:    
            query = 'SELECT question, answer_start, answer_string FROM question_answer WHERE article_id = ?'
            with self.__conn:
                cursor = self.__conn.cursor()
                cursor.execute(query, (articleId,))
                result = cursor.fetchone()
        return result


    def getQuestionAndAnswerById(self, idx:int) -> (str, int, str):
        if idx > self.__maxQuestionAndAnswerNumber:
            result = ('NULL', -1, 'NULL')
        else:    
            query = 'SELECT question, answer_start, answer_string FROM question_answer WHERE id = ?'
            with self.__conn:
                cursor = self.__conn.cursor()
                cursor.execute(query, (idx,))
                result = cursor.fetchone()[0]
        return result

    def getAllQuestionAndAnswer(self):
        query = 'SELECT question, answer_start, answer_string FROM question_answer'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query)
            result = cursor.fetchall()
        return result