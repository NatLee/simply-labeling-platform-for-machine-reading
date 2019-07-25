import datetime

from flask import Flask, render_template, request, jsonify
from utils.databaseTool import MrcDatabase

app = Flask(__name__)
mcd = MrcDatabase('mrc.label')


@app.route('/getArticle', methods=['POST'])
def getArticle():
    articleId, article = mcd.getRandomArticle()
    return jsonify({'article_id': articleId, 'article':article, 'timestamp': datetime.datetime.now().replace(tzinfo=datetime.timezone(datetime.timedelta(hours=8))).isoformat()})

@app.route('/insertQuestionAnswer', methods=['POST'])
def insertQuestionAnswer():
    questionAnswers = request.get_json()
    for questionAnswer in questionAnswers:
        articleId = questionAnswer.get('article_id')
        question = questionAnswer.get('question')
        ansStart = questionAnswer.get('answer_start')
        if articleId is  None or question is None or ansStart is None:
            continue
        else:
            mcd.insertQuestionAnswer(articleId, question, ansStart)


@app.route('/', methods=['POST', 'GET'])
def index():
    return render_template('index.html')


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=8787, debug=True)


