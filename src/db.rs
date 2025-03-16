use bonsaidb::core::connection::AsyncConnection;
use bonsaidb::core::document::Emit;
use bonsaidb::core::schema::{self, Collection, CollectionMapReduce, View, ViewSchema};
use bonsaidb::local::AsyncDatabase;
use bonsaidb::local::config::{Builder, StorageConfiguration};
use serde::*;
use time::OffsetDateTime;

type Result<T, E = bonsaidb::core::Error> = std::result::Result<T, E>;

pub struct DB(AsyncDatabase);

#[derive(Deserialize, Serialize, Collection, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[collection(name = "highscore", views = [ScoreAndTime, HighscoreByUser])]
pub struct Highscore {
    pub userid: String,
    pub username: String,
    pub score: u64,
    #[serde(with = "time::serde::rfc3339", default = "OffsetDateTime::now_utc")]
    pub created_at: OffsetDateTime,
    #[serde(default)]
    pub place: usize,
}

#[derive(Debug, Clone, View, ViewSchema)]
#[view(collection = Highscore, key = (u64, i128), value = usize, name = "score-and-time")]
#[view_schema(version = 2)]
struct ScoreAndTime;

impl CollectionMapReduce for ScoreAndTime {
    fn map<'doc>(
        &self,
        document: bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>,
    ) -> schema::ViewMapResult<'doc, Self>
    where
        bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>: 'doc,
    {
        document.header.emit_key_and_value(
            (
                document.contents.score,
                -(document.contents.created_at.unix_timestamp_nanos()),
            ),
            1,
        )
    }

    fn reduce(
        &self,
        mappings: &[schema::ViewMappedValue<'_, Self>],
        _rereduce: bool,
    ) -> schema::ReduceResult<Self::View> {
        Ok(mappings.iter().map(|c| c.value).sum())
    }
}

#[derive(Debug, Clone, View, ViewSchema)]
#[view(collection = Highscore, key = String, name = "highscore-by-user")]
struct HighscoreByUser;

impl CollectionMapReduce for HighscoreByUser {
    fn map<'doc>(
        &self,
        document: bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>,
    ) -> schema::ViewMapResult<'doc, Self>
    where
        bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>: 'doc,
    {
        document.header.emit_key(document.contents.userid)
    }
}

#[derive(Debug, schema::Schema)]
#[schema(name = "server", collections=[Highscore])]
struct Schema;

impl DB {
    pub async fn new() -> Result<Self> {
        Ok(Self(
            AsyncDatabase::open::<Schema>(StorageConfiguration::new("data.bonsaidb")).await?,
        ))
    }

    pub async fn get_top(&self) -> Result<Vec<Highscore>> {
        Ok(self
            .0
            .view::<ScoreAndTime>()
            .descending()
            .limit(20)
            .query_with_collection_docs()
            .await?
            .into_iter()
            .enumerate()
            .map(|(i, d)| {
                let mut d = d.document.contents.clone();
                d.place = i + 1;
                d
            })
            .collect())
    }

    pub async fn get_around(&self, userid: String) -> Result<Vec<Highscore>> {
        let highscore_by_user = self
            .0
            .view::<HighscoreByUser>()
            .with_key(&userid)
            .query_with_collection_docs()
            .await?;
        if !highscore_by_user.is_empty() {
            let score = highscore_by_user.into_iter().next().unwrap();
            let highest_score_for_user = (
                score.document.contents.score,
                -score.document.contents.created_at.unix_timestamp_nanos(),
            );
            let user_place = self
                .0
                .view::<ScoreAndTime>()
                .with_key_range(highest_score_for_user..)
                .reduce()
                .await?;
            let mut before: Vec<_> = self
                .0
                .view::<ScoreAndTime>()
                .ascending()
                .with_key_range(highest_score_for_user..)
                .limit(10)
                .query_with_collection_docs()
                .await?
                .into_iter()
                .map(|d| d.document.contents.clone())
                .collect();
            for (i, score) in before.iter_mut().enumerate() {
                score.place = user_place - i;
            }
            before.reverse();
            let after: Vec<_> = self
                .0
                .view::<ScoreAndTime>()
                .descending()
                .with_key_range(..highest_score_for_user)
                .limit(10)
                .query_with_collection_docs()
                .await?
                .into_iter()
                .enumerate()
                .map(|(i, d)| {
                    let mut d = d.document.contents.clone();
                    d.place = user_place + i + 1;
                    d
                })
                .collect();
            Ok(before.into_iter().chain(after).collect())
        } else {
            self.get_top().await
        }
    }

    pub async fn insert(&self, highscore: Highscore) -> Result<()> {
        let existing = self
            .0
            .view::<HighscoreByUser>()
            .with_key(&highscore.userid)
            .query_with_collection_docs()
            .await?;
        if let Some(existing) = existing.into_iter().next() {
            if highscore.score > existing.document.contents.score {
                existing
                    .document
                    .clone()
                    .modify_async(&self.0, |m| {
                        m.contents = highscore.clone();
                    })
                    .await?;
            }
        } else {
            self.0.collection::<Highscore>().push(&highscore).await?;
        }
        Ok(())
    }

    pub async fn delete(&self, user: String) -> Result<()> {
        self.0
            .view::<HighscoreByUser>()
            .with_key(&user)
            .delete_docs()
            .await?;
        Ok(())
    }
}
