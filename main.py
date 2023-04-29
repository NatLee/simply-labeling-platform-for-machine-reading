import datetime
import json
from flask import Flask, render_template, request, jsonify
from utils import MrcDatabase

app = Flask(__name__)
db = MrcDatabase('mrc.label')

@app.route('/article', methods=['GET'])
def get_article():
    articleId, article = db.getRandomArticle()
    return jsonify({'article_id': articleId, 'article': article, 'timestamp': datetime.datetime.now().replace(tzinfo=datetime.timezone(datetime.timedelta(hours=8))).isoformat()})

@app.route("/article", methods=["POST"])
def insert_article():
    article = request.form.get("article")
    description = request.form.get("description")

    if article: # only check field of article
        db.insertArticle(article, description)
        return {"status": "success", "message": "Article inserted successfully."}, 200

    return {"status": "error", "message": "Article or description is missing."}, 400

@app.route('/question-answer', methods=['POST'])
def insert_question_and_answer():
    questionAnswers = request.get_json()
    for questionAnswer in questionAnswers:
        articleId = questionAnswer.get('article_id')
        question  = questionAnswer.get('question')
        ansStart  = questionAnswer.get('answer_start')
        ansString = questionAnswer.get('answer_string')
        if articleId is  None or question is None or ansStart is None:
            continue
        else:
            db.insertQuestionAnswer(articleId, question, ansStart, ansString)
    return json.dumps({'success': True}), 200, {'ContentType': 'application/json'}


@app.route('/', methods=['POST', 'GET'])
def index():
    return render_template('index.html')


if __name__ == '__main__':
    app.run(host='0.0.0.0', port=8787, debug=True)
