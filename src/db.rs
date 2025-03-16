use bonsaidb::core::connection::AsyncConnection;
use bonsaidb::core::document::Emit;
use bonsaidb::core::schema::{
    self, Collection, CollectionMapReduce, SerializedView, View, ViewSchema,
};
use bonsaidb::local::AsyncDatabase;
use bonsaidb::local::config::{Builder, StorageConfiguration};
use serde::*;
use time::OffsetDateTime;

type Result<T, E = bonsaidb::core::Error> = std::result::Result<T, E>;

pub struct DB(AsyncDatabase);

#[derive(Deserialize, Serialize, Collection, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[collection(name = "highscore", views = [ScoreAndTime, HighscoreForUser])]
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
#[view(collection = Highscore, key = String, value=(u64, i128), name = "highscore-for-user2")]
struct HighscoreForUser;

impl CollectionMapReduce for HighscoreForUser {
    fn map<'doc>(
        &self,
        document: bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>,
    ) -> schema::ViewMapResult<'doc, Self>
    where
        bonsaidb::core::document::CollectionDocument<<Self::View as View>::Collection>: 'doc,
    {
        document.header.emit_key_and_value(
            document.contents.userid,
            (
                document.contents.score,
                -(document.contents.created_at.unix_timestamp_nanos()),
            ),
        )
    }

    fn reduce(
        &self,
        mappings: &[schema::ViewMappedValue<'_, Self>],
        _rereduce: bool,
    ) -> schema::ReduceResult<Self::View> {
        Ok(mappings.iter().map(|v| v.value).max().unwrap_or_default())
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
        Ok(ScoreAndTime::entries_async(&self.0)
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
        if !HighscoreForUser::entries_async(&self.0)
            .with_key(&userid)
            .limit(1)
            .query()
            .await?
            .is_empty()
        {
            let highest_score_for_user = HighscoreForUser::entries_async(&self.0)
                .with_key(&userid)
                .reduce()
                .await?;
            let user_place = ScoreAndTime::entries_async(&self.0)
                .with_key_range(highest_score_for_user..)
                .reduce()
                .await?;
            let mut before: Vec<_> = ScoreAndTime::entries_async(&self.0)
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
            let after: Vec<_> = ScoreAndTime::entries_async(&self.0)
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
        self.0.collection::<Highscore>().push(&highscore).await?;
        Ok(())
    }
}
