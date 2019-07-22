var sampleText = "對於歐盟此草案的第11條與第13條引發的爭議較高，而第13條爭議最大。\
2016年《數位單一市場著作權指令》草案剛剛出台時，音樂產業的相關團體對此表示歡迎。國際唱片業協會指出這是消融向線上服務與著作權所有者鴻溝的「第一步」。Google稱該草案有一些積極的東西，但對其暗示的網際網路審查表示了擔憂。Musically指出，一些人士認為YouTube、SoundCloud這樣的網站不應成為「安全避風港」，並非常歡迎該草案的出台。同年10月10日，中國大陸媒體人民網援引《中國知識產權報》評論稱，該草案對數位環境下中國著作權法的立法完善具有一定的借鑑意義。\
起草該條款的關鍵人物之一、德國議員阿克塞爾·沃斯稱，該法案只會影響到那些利用著作權材料在賺錢的人。歐洲議會中也有諸多指令支持者。"

function renderSampleText(){
    var html=$("#passage").val(sampleText);
}

var passageSelectionStart = 0
var passageSelectionEnd = 0

$(document).ready(function(){
    $("#passage").val(sampleText);
    $('#passage').select(function(e) {
        passageSelectionStart = e.target.selectionStart;
        passageSelectionEnd = e.target.selectionEnd;
        message = '' + passageSelectionStart + '-' + passageSelectionEnd
        $("#index").empty()
        $("#index").append(message)
        $("#index").css("color", '#000000');
    });
    $("#passage").mousedown(function(event){
        passageSelectionStart = 0
        passageSelectionEnd = 0
    });
    $("#passage").mouseup(function(event){
        message = '' + passageSelectionStart + '-' + passageSelectionEnd
        $("#index").css("color", '#00CC66');
        console.log(message)
    });
  });