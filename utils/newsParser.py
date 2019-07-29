from pathlib import Path
from tqdm import tqdm
import json
import shutil

from utils.databaseTool import MrcDatabase 

class newsImport():

    def __init__(self, newsPath:str, mcd:MrcDatabase):

        self.newsPairs = self.__getAllNews(newsPath)
        for news in tqdm(self.newsPairs):
            content, category = news
            mcd.insertArticle(content, category)
            mcd.commit()

    def __getAllNews(self, newsPath:str):
        newsPath = Path(newsPath)
        newsFiles = newsPath.glob('*/*')
        allNews = list()

        processedDir = newsPath / 'processed'
        if not processedDir.exists():
            processedDir.mkdir()

        pbar = tqdm(newsFiles, desc='News processing ...')
        for newsFile in pbar:
            path = newsFile.absolute().as_posix()
            newPathFolder = processedDir / newsFile.parents[0].name
            if not newPathFolder.exists():
                newPathFolder.mkdir()
            newPath = newPathFolder / newsFile.name
            if path.find(processedDir.absolute().as_posix()) < 0 and path.find('.DS_Store') < 0 : 

                with open(newsFile, 'r', encoding='utf-8') as f:
                    newsRaw = f.read()
                    if newsRaw != '':
                        newsJson = json.loads(newsRaw)
                        for news in newsJson:
                            if len(news.get('content')) > 100:
                                allNews.append(news)
                        shutil.move(path, newPath)

        newsPairs = list()

        for news in allNews:
            newsPairs.append((news.get('content'), news.get('category'),))

        return newsPairs
