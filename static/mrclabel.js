var sampleText = "對於歐盟此草案的第11條與第13條引發的爭議較高，而第13條爭議最大。\
2016年《數位單一市場著作權指令》草案剛剛出台時，音樂產業的相關團體對此表示歡迎。國際唱片業協會指出這是消融向線上服務與著作權所有者鴻溝的「第一步」。Google稱該草案有一些積極的東西，但對其暗示的網際網路審查表示了擔憂。Musically指出，一些人士認為YouTube、SoundCloud這樣的網站不應成為「安全避風港」，並非常歡迎該草案的出台。同年10月10日，中國大陸媒體人民網援引《中國知識產權報》評論稱，該草案對數位環境下中國著作權法的立法完善具有一定的借鑑意義。\
起草該條款的關鍵人物之一、德國議員阿克塞爾·沃斯稱，該法案只會影響到那些利用著作權材料在賺錢的人。歐洲議會中也有諸多指令支持者。"


var passageSelectionStart = 0;
var passageSelectionEnd = 0;
var focusId = 0;
var rowCount = 0;
var ansIndex = [];
var articleId = 0;
$(document).ready(function(){
    // Create placeholder for answer index.
    var ansObj = {"start": 0, "end": 0, "text": 0};

    ansIndex.push(ansObj);
    // Create first row and focus.
    newRow(rowCount); focusId=rowCount; rowCount++;

    // Setup paragraph.
    $.post('/getArticle', function(data) {
        $('#passage').val(data.article);
        articleId = data.article_id;
    });

    // Setup events.
    $('#passage').select(function(e) {
        let passage = $("#passage").val();
        let msgTarget = "#index-" + focusId;
        let textTarget = "#answer-" + focusId;

        passageSelectionStart = e.target.selectionStart;
        passageSelectionEnd = e.target.selectionEnd;
        ansIndex[focusId]['start'] = passageSelectionStart;
        ansIndex[focusId]['end'] = passageSelectionEnd;

        let selectedText = passage.substring(passageSelectionStart, passageSelectionEnd)
        ansIndex[focusId]['text'] = selectedText;

        message = '' + passageSelectionStart + '-' + passageSelectionEnd
        $(msgTarget).empty()
        $(msgTarget).append(message)
    });

    $('#sendBtn').click(function(e){
        generateQA(ansIndex);
    });

  });

function newRow(id){
    let rowParent = $('<div />', {"class": 'row p-1'});
    let colParent = $('<div />', {"class": 'offset-md-2 col-md-8'});
    let flexContainer = $('<div />', {"class": 'd-flex'});

    // Add-row button placeholder.
    let btnAddCont = $('<div />', {"class": 'p-2 flex-shrink-1 bg-li'});
    btnAddCont.append($('<button type="button" class="btn  bg-primary" id="addbtn-' + id + '">+</button>').click(addBtnEventHandler));

    // Modify button placeholder.
    let btnModCont = $('<div />', {"class": 'p-2 flex-shrink-1 bg-li'});
    btnModCont.append($('<button type="button" class="btn  bg-warning" id="addbtn-' + id + '">M</button>').click(modBtnEventHandler));

    let qboxCont = $('<div />', {"class": 'p-2 flex-fill bg-secondary'});
    qboxCont.append($('<input type="text" class="form-control" id="question-' + id + '" placeholder="Question..." />'));

    //let aboxCont = $('<div />', {"class": 'p-2 flex-fill bg-success'});
    //aboxCont.append($('<input type="text" class="form-control" id="answer-' + id + '" placeholder="Generated Answer" />'));

    let labelCont = $('<div />', {"class": 'p-2 flex-shrink-10 bg-success', "style": "width: 100px;"});
    labelCont.append($('<label id="index-' + id + '">n/a</label>'));

    flexContainer.append(btnAddCont);
    flexContainer.append(btnModCont);
    flexContainer.append(qboxCont);
    //flexContainer.append(aboxCont);
    flexContainer.append(labelCont);

    colParent.append(flexContainer);
    rowParent.append(colParent);
    $('#qarow').append(rowParent);
}

function addBtnEventHandler(ev){
    var ansObj = {"start": 0, "end": 0};
    ansIndex.push(ansObj);

    focusId = rowCount;
    newRow(rowCount);
    rowCount = rowCount + 1;
}

/**
 * Event handler of modify button.
 * @param {*} ev Event object
 */
function modBtnEventHandler(ev){
    focusId = Number(ev.target.id.split('-')[1]);
}

function generateQA(rawQAPair){
    for(let i = 0; i < rowCount; i++) {
        let qText = $("#question-" + i).val();
        rawQAPair[i]['article_id'] = articleId;
        rawQAPair[i]['answer_start'] = rawQAPair[i]['start'];
        rawQAPair[i]['answer_string'] = rawQAPair[i]['text'];
        rawQAPair[i]['question'] = qText;
    }

    // Push json to backend.
    console.log(rawQAPair);
    $.ajax({  
        type: "POST",
        url: '/insertQuestionAnswer',
        data: JSON.stringify(rawQAPair),
        contentType: "application/json",
        success: function(data){
            console.log(data)
        }
    });
}