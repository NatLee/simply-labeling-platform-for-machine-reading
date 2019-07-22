
from flask import Flask, render_template

app = Flask(__name__)


@app.route('/', methods=['POST', 'GET'])
def index():
    yee = 'Guten Tag.'
    return render_template('index.html')


if __name__ == "__main__":
    app.run(host='0.0.0.0', port=8787, debug=True)


