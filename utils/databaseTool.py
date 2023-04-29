import sqlite3
import datetime
import random
from pathlib import Path

class MrcDatabase(object):
    """MRC Database SQLite toolkit.

    Attributes:
        sqlitePath (str): The sqlite file path.

    """

    ARTICLE_DATABASE = """CREATE TABLE "article" (
	                        "id"	INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT UNIQUE,
	                        "context"	TEXT NOT NULL,
	                        "timestamp"	TEXT NOT NULL,
	                        "description"	TEXT
                        );"""

    ANSWER_QUESTION_DATABASE = """CREATE TABLE "question_answer" (
	                        "id"	INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT UNIQUE,
	                        "article_id"	INTEGER NOT NULL,
                            "question"	TEXT NOT NULL,
                            "answer_start"	INTEGER NOT NULL,
                            "answer_string"	TEXT NOT NULL,
                            "timestamp"	TEXT NOT NULL
                        );"""

    def __init__(self, sqlitePath:str):

        self.__timezone = datetime.timezone(datetime.timedelta(hours=8))
        self.__conn = self.__buildDatabase(sqlitePath)
        self.__maxArticleNumber = self.__getMaxArticleNumber()
        self.__maxQuestionAndAnswerNumber = self.__getMaxQuestionAndAnswerNumber()

    def __buildDatabase(self, sqlitePath:str):
        if Path(sqlitePath).exists():
            conn = sqlite3.connect(sqlitePath, check_same_thread = False)
        else:
            conn = sqlite3.connect(sqlitePath, check_same_thread = False)
            with conn:
                cursor = conn.cursor()
                cursor.execute(self.ARTICLE_DATABASE)
                cursor.execute(self.ANSWER_QUESTION_DATABASE)
        return conn

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

    def insertArticle(self, article:str, description:str):
        query = 'INSERT into article (context, timestamp, description) VALUES (?, ?, ?);'
        timestamp = datetime.datetime.now().replace(tzinfo=self.__timezone).isoformat()
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query, (article, timestamp, description))
        self.__maxArticleNumber += 1

    def getAllArticleID(self) -> int:
        if self.__maxArticleNumber == 0:
            return []

        query = 'SELECT id FROM article'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query)
            result = cursor.fetchall()
            if result:
                return [res[0] for res in result]
            else:
                self.__maxArticleNumber = 0
                return []

    def getRandomArticle(self) -> (int, str):
        if self.__maxArticleNumber == 0:
            return None, None

        ids = self.getAllArticleID()
        if ids:
            idx = random.choice(ids)
        else:
            return None, None
        query = 'SELECT id, context FROM article WHERE id = ?'
        with self.__conn:
            cursor = self.__conn.cursor()
            cursor.execute(query, (idx,))
            result = cursor.fetchone()
            if result:
                article_id, context = result
            else:
                return self.getRandomArticle()

        return article_id, context


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