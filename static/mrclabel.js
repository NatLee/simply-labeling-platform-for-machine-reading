var focusId = 0;
var ansIndex = [];
var articleId = 0;

$(document).ready(function() {
  init();
});

function init() {
  ansIndex.push({ start: 0, end: 0, text: 0 });

  newRow();
  getNewArticle();
  setPassageEvent();

  $("#submit-article").click(submitArticle);
  $("#switch-article").click(switchArticle);
  $("#sendBtn").click(function() {
    generateQA(ansIndex);
  });
}

function getRowCount() {
    return $('.row.p-1').length;
}

function setPassageEvent() {

    let passageSelectionStart = 0;
    let passageSelectionEnd = 0;

    // stop event
    $("#passage").off('select');
    // start event
    $("#passage").select(function (e) {
        focusId = getRowCount() - 1;

        let passage = $("#passage").val();
        let msgTarget = ".index-label:eq(" + focusId + ")";
        let textTarget = ".answer-box:eq(" + focusId + ")";

        passageSelectionStart = e.target.selectionStart;
        passageSelectionEnd = e.target.selectionEnd;

        ansIndex[focusId]["start"] = passageSelectionStart;
        ansIndex[focusId]["end"] = passageSelectionEnd;

        let selectedText = passage.substring(
            passageSelectionStart,
            passageSelectionEnd
        );

        ansIndex[focusId]["text"] = selectedText;

        message = "" + passageSelectionStart + "-" + passageSelectionEnd;
        $(msgTarget).empty();
        $(msgTarget).append(message);

        $(textTarget).val(selectedText);

        // Unselect the text
        window.getSelection().removeAllRanges();

    });
}

function getNewArticle() {
    // Setup paragraph.
    $.get('/article', function (data) {
        $('#passage').val(data.article);
        articleId = data.article_id;
    });
}

function switchArticle() {
    focusId = 0;
    ansIndex = [];
    articleId = 0;
    getNewArticle();
    flushQA();
    setPassageEvent();
}

function newRow() {
    let rowParent = $('<div />', {
        "class": 'row p-1'
    });
    let colParent = $('<div />', {
        "class": 'offset-md-2 col-md-8'
    });
    let flexContainer = $('<div />', {
        "class": 'd-flex'
    });
    
    // Add-row button placeholder.
    let btnAddCont = $('<div />', {
        "class": 'p-2 flex-shrink-1 bg-li'
    });
    // Remove-row button placeholder.
    let btnDelCont = $('<div />', {
        "class": 'p-2 flex-shrink-1 bg-li'
    });
    // Modify button placeholder.
    let btnModCont = $('<div />', {
        "class": 'p-2 flex-shrink-1 bg-li'
    });
    let qboxCont = $('<div />', {"class": 'p-2 flex-fill bg-secondary'});
    let aboxCont = $('<div />', {"class": 'p-2 flex-fill bg-success'});
    let labelCont = $('<div />', {
        "class": 'p-2 flex-shrink-10 bg-success',
        "style": "width: 100px;"
    });

    // Add-row button
    btnAddCont.append($('<button type="button" class="btn  bg-primary add-btn">+</button>').click(addBtnEventHandler));

    // Remove-row button
    btnDelCont.append($('<button type="button" class="btn bg-danger del-btn">-</button>').click(delBtnEventHandler));

    // question box
    qboxCont.append($('<input type="text" class="form-control question-box" placeholder="Your Question" />'));

    // answer box
    aboxCont.append($('<input readonly type="text" class="form-control answer-box" placeholder="Selected Answer" />'));

    // label
    labelCont.append($('<label class="index-label">n/a</label>'));


    flexContainer.append(btnAddCont);
    flexContainer.append(btnDelCont);
    flexContainer.append(btnModCont);
    flexContainer.append(qboxCont);
    flexContainer.append(aboxCont);
    flexContainer.append(labelCont);

    colParent.append(flexContainer);
    rowParent.append(colParent);
    $('#qarow').append(rowParent);
}

function addBtnEventHandler(ev) {
    // Check if any row has no selected text
    let anyRowNotSelected = ansIndex.some((ansObj) => ansObj.start === 0 && ansObj.end === 0);
  
    if (anyRowNotSelected) {
      Swal.fire({
        icon: "warning",
        title: "Oops...",
        text: "請先選擇每個row的文本。",
      });
    } else {
      var ansObj = { start: 0, end: 0, text:0};
      ansIndex.push(ansObj);

      newRow();
      reassignIds();
      focusId = getRowCount() - 1;
    }
  }

function flushQA() {
    // Clear all rows except the first one
    $(".row.p-1").not(":first").remove();
    ansIndex = [{"start": 0, "end": 0, "text": 0}];
    // Clear the remaining row's inputs
    $("#question-0").val("");
    $("#answer-0").val("");
    $("#index-0").text("n/a");
}

function generateQA(rawQAPair) {
    for (let i = 0; i < getRowCount(); i++) {
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
        url: '/question-answer',
        data: JSON.stringify(rawQAPair),
        contentType: "application/json",
        success: function (data) {
            flushQA();
            Swal.fire({
                icon: 'success',
                title: '成功',
                text: '問題及答案已送出！',
            });
            console.log(data);
        },
        error: function (xhr, status, error) {
            Swal.fire({
                icon: 'error',
                title: '錯誤',
                text: '出現問題！',
            });
            console.log("Error: " + error);
        }
    });
}

/**
 * Event handler of delete button.
 * @param {*} ev Event object
 */
function delBtnEventHandler(ev) {
    let rowId = Number($(ev.target).data("row-id"));
    let rowCount = getRowCount();

    if (rowCount > 1) {
        let row = $(ev.target).closest(".row");
        row.remove();
        ansIndex.splice(rowId, 1);
        reassignIds();
        focusId = getRowCount() - 1;

    } else {
        Swal.fire({
            icon: "error",
            title: "無法刪除",
            text: "無法刪除最後一行",
        });
    }
}


function submitArticle() {
    // Get form data
    let article = $("#article").val();
    let description = $("#description").val();

    // Send the data using an AJAX POST request
    $.ajax({
        type: "POST",
        url: "/article",
        data: {
            article: article,
            description: description,
        },
        success: function (response) {
            $("#article").val("");
            $("#description").val("");
            Swal.fire({
                icon: "success",
                title: "成功",
                text: "文章已成功新增！",
            });
        },
        error: function (xhr, status, error) {
            Swal.fire({
                icon: "error",
                title: "錯誤",
                text: "添加文章時出現問題！",
            });
        },
    });
}


function reassignIds() {
    $('.row.p-1').each(function (index, row) {
        let btnAdd = $(row).find('.btn-add');
        let btnDel = $(row).find('.btn-del');
        let btnMod = $(row).find('.btn-mod');
        let questionBox = $(row).find('.question-box');
        let answerBox = $(row).find('.answer-box');
        let indexLabel = $(row).find('.index-label');

        btnAdd.attr('id', 'addbtn-' + index);
        btnDel.attr('id', 'delbtn-' + index);
        btnMod.attr('id', 'addbtn-' + index);
        questionBox.attr('id', 'question-' + index);
        answerBox.attr('id', 'answer-' + index);
        indexLabel.attr('id', 'index-' + index);
    });
}
