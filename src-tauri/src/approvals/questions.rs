use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Choice {
    label: String,
    #[serde(default)]
    description: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Question {
    id: String,
    question: String,
    #[serde(default)]
    header: String,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    options: Vec<Choice>,
    #[serde(default)]
    multi_select: bool,
    #[serde(default)]
    fixed: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Response {
    id: String,
    selected: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom: Option<String>,
}

pub(super) fn valid_questions(questions: &[Question]) -> bool {
    let mut ids = HashSet::new();
    !questions.is_empty() && questions.iter().all(|q| {
        let mut labels = HashSet::new();
        !q.id.trim().is_empty() && ids.insert(&q.id) && !q.question.trim().is_empty()
            && q.options.iter().all(|opt| !opt.label.trim().is_empty() && labels.insert(&opt.label))
            && (!q.fixed || !q.options.is_empty())
    })
}

pub(super) fn valid_answers(questions: &[Question], answers: &[Response]) -> bool {
    answers.len() == questions.len() && questions.iter().zip(answers).all(|(q, a)| {
        let mut labels = HashSet::new();
        let custom = a.custom.as_ref().is_some_and(|s| !s.trim().is_empty());
        q.id == a.id && (q.multi_select || a.selected.len() + usize::from(custom) <= 1)
            && (!q.fixed || (a.selected.len() == 1 && a.custom.is_none()))
            && a.selected.iter().all(|label| labels.insert(label) && q.options.iter().any(|opt| &opt.label == label))
    })
}
