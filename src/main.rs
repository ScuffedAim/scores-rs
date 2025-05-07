extern crate tokio;

use discord_webhook2::message::Message;
use discord_webhook2::webhook::DiscordWebhook;
// use discord_webhook2::message::embed::field::EmbedField;
use dotenvy::dotenv;
use rosu_v2::prelude::*;
use core::time;
use std::collections::HashMap;
use std::os::unix::process;
use std::{env, path};

use reqwest::Client;

use chrono::DateTime;
use std::sync::Arc;
mod scuffedaim;

struct OsuResources {
    grades: HashMap<String, String>,
    grade_colors: HashMap<String, u32>,
}
impl OsuResources {
    fn new() -> Self {
        let grades = HashMap::from([
            (
                "SSH".to_string(),
                "https://i.ibb.co/SKWVrN4/ranking-XH-2x.png".to_string(),
            ),
            (
                "SS".to_string(),
                "https://i.ibb.co/d6n0kZW/Ranking-X-2x.png".to_string(),
            ),
            (
                "SH".to_string(),
                "https://i.ibb.co/2KRz0z2/ranking-SH-2x.png".to_string(),
            ),
            (
                "S".to_string(),
                "https://i.ibb.co/X8zgd8t/Ranking-S-2x.png".to_string(),
            ),
            (
                "A".to_string(),
                "https://i.ibb.co/mNcTptM/Ranking-A-2x.png".to_string(),
            ),
            (
                "B".to_string(),
                "https://i.ibb.co/cL18ZXP/Ranking-B-2x.png".to_string(),
            ),
            (
                "C".to_string(),
                "https://i.ibb.co/R3QZ0PL/Ranking-C-2x.png".to_string(),
            ),
            (
                "D".to_string(),
                "https://i.ibb.co/sjNbs7V/Ranking-D-2x.png".to_string(),
            ),
        ]);
        let grade_colors = HashMap::from([
            ("SSH".to_string(), 0xC0C0C0),
            ("SS".to_string(), 0xF5F245),
            ("SH".to_string(), 0xC0C0C0),
            ("S".to_string(), 0xF5F245),
            ("A".to_string(), 0x41CF34),
            ("B".to_string(), 0x5B86CF),
            ("C".to_string(), 0x9331DE),
            ("D".to_string(), 0xEB3150),
        ]);


        OsuResources {
            grades,
            grade_colors,
        }
    }
}

async fn get_recent_scores(osu: &Osu,user_id: UserId) -> Vec<Score> {
    osu.user_scores(user_id).mode(GameMode::Osu).recent().await.unwrap()
}

async fn get_members(api_url: &str, token: &str) -> Result<Vec<scuffedaim::Member>, reqwest::Error> {
    let client = Client::new();
    let response = client.request(reqwest::Method::GET, api_url)
        .header("Authorization", token)
        .send()
        .await?;
    let members = response.json::<Vec<scuffedaim::Member>>().await?;
    Ok(members)
}



async fn load_processed_scores() -> Vec<u64> {

    let file = std::fs::File::open("processed_scores.json").unwrap();
    let reader = std::io::BufReader::new(file);
    let scores: Vec<u64> = serde_json::from_reader(reader).unwrap();
    scores
}
async fn save_processed_scores(scores: Vec<u64>) {
    let file = std::fs::File::create("processed_scores.json").unwrap();
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer(writer, &scores).unwrap();
}

async fn send_discord(osu: &Osu,score:Score) {
    let pp_threshold = (score.pp.unwrap_or(0.0) / 100.0 ).trunc() * 100.0;
    let webhook_url = env::var(format!("PP{}_WEBHOOK_URL", pp_threshold as i32)).unwrap_or_else(|_| "".to_string());
    if webhook_url.is_empty() {
        println!("No webhook url found for {}PP", pp_threshold);
        return;
    }
    let map = match score.map {
        Some(ref map) => map,
        None => {println!("No map found for score {}", score.id); return;},
    };
    let mapset = match &map.mapset {
        Some(mapset) => &**mapset,
        None => &osu.beatmapset(map.mapset_id).await.unwrap(),
    };
    let user = match score.user {
        Some(ref user) => user,
        None => {println!("No user found for score {}", score.id); return;},
    };
    let user_statistics = match user.statistics {
        Some(ref statistics) => statistics,
        None => &osu.user(user.user_id).await.unwrap().statistics.unwrap(),
    };
    
    let osu_resources = OsuResources::new();

    let grade = score.grade.to_string();
    let grade_color = osu_resources.grade_colors.get(&grade).unwrap_or(&0xFFFFFF);
    let grade_image = osu_resources.grades.get(&grade).unwrap();

    let mods = score.mods.as_legacy().to_string();


    let map_title = format!("{} [{}]",mapset.clone().title_unicode.unwrap_or("".to_string()),map.version);
    let map_image = mapset.clone().covers.cover_2x;
    let map_url = format!("https://osu.ppy.sh/beatmapsets/{}/#osu/{}", mapset.mapset_id, map.map_id);
    let player_url = format!("https://osu.ppy.sh/users/{}", score.user_id);
    

    let player_name = &user.username.to_string();
    let player_image = &user.avatar_url;
    let player_rank = user_statistics.global_rank.unwrap_or(0);
    let country_rank = user_statistics.country_rank.unwrap_or(0);
    let country_code = &user.country_code;
    let score_url = format!("https://osu.ppy.sh/scores/{}",score.id);
    
    let difficulty_attributes = osu.beatmap_difficulty_attributes(score.map_id).mode(GameMode::Osu).mods(score.mods.clone()).await.unwrap();
    
    // scale ar, od, bpm, based on mods
    
    let clock_rate = match score.mods.clock_rate() {
        Some(rate) => rate,
        None => 1.0,
    } as f32;
    let bpm = map.bpm * clock_rate;
    let mut ar = map.ar;
    let mut od = map.od;
    let mut cs = map.cs;
    let mut hp = map.hp;

    if score.mods.contains_acronym("HR".parse().unwrap()) {
        // HR
        hp = (hp*1.4).min(10.0);
        cs *= 1.3;
        ar *= 1.4;
        hp *= 1.4;
        od *= 1.4;
    }
    else if score.mods.contains_acronym("EZ".parse().unwrap()) {
        // EZ
        cs *= 0.5;
        hp *= 0.5;
        od *= 0.5;
        ar *= 0.5;
    }
    // TODO: sprav toto ale aj zos ar a od a hp neviem čo ešte pls ok dik
    

    let webhook = DiscordWebhook::new(&webhook_url).unwrap();
    let message = Message::new(|message| {
        
        message.avatar_url(player_image)
                .username(player_name.clone().to_string())
                .embed(|embed| {
                    let score_url = score_url.clone();
                    let player_name = player_name.clone();
                    let score = score.clone();
                    let map = map.clone();
                    embed
                        .title(map_title)
                        .url(&map_url)
                        .description(
                            format!(
                            "╰┈➤ Playerׂ
> **Rank**: #{} ({}{})

ׂ╰┈➤ Scoreׂ
> {}
> **{}PP** {}%
> **{}**/{}x
> {} <:100:1307101924436344852> / {} <:50:1307101926089031690> / {} <:miss:1307101928718864484>

ׂ╰┈➤ Beatmapׂ
 ```
 CS {} | AR {} | OD {} | HP {}
 {} BPM - {} ⭐ - {}
 ```    

[beatmap]({}) • [player]({}) • [score]({})"
                            ,player_rank, country_code, country_rank,
                            mods,
                            score.pp.unwrap_or(0.0),
                            score.clone().legacy_accuracy(),
                            score.max_combo,
                            difficulty_attributes.max_combo,
                            score.statistics.ok,
                            score.statistics.meh,
                            score.statistics.miss,
                            cs,
                            ar,
                            od,
                            hp,
                            bpm,
                            difficulty_attributes.stars,
                            DateTime::from_timestamp_millis(((map.seconds_drain as f32/clock_rate*1000.0) as u32).into()).unwrap().format("%M:%S"),
                            map_url,
                            player_url,
                            &score_url
                            ))    
                        .color(grade_color.clone())
                        .thumbnail(|thumbnail| {
                            thumbnail
                                .url(grade_image)
                        
                        })
                        .image(|image| {
                            image
                                .url(map_image)
                        })
                        .author(|author| {
                            author
                                .name(player_name)
                                .url(player_url)
                        })
                        .footer(|footer| {
                            footer
                                .text("Scoreposter made by sneznykocur, original idea from reinum <3")
                        })
                        
                    
                })
        }
    );
    webhook.send(&message).await.unwrap();
}

#[tokio::main]
async fn main() -> () {
    dotenv().ok();
    let api_key = env::var("API_KEY").unwrap();
    let api_url = "http://localhost:3000/members";
    let osu = Arc::new(Osu::new(env::var("CLIENT_ID").unwrap().parse().unwrap(), env::var("CLIENT_SECRET").unwrap()).await.unwrap());
    let processed_scores = load_processed_scores().await;
    loop {
        let members: Vec<_> = get_members(api_url, &api_key).await.unwrap();
        let members = members.into_iter().filter(|member| member.user_id.is_some());
        println!("Found {} members", members.clone().collect::<Vec<_>>().len());
        for member in members {
            let member = member.clone();
            let osu = osu.clone();
            let mut processed_scores = processed_scores.clone();
            println!("processed {} scores", processed_scores.len());
            tokio::spawn(async move {
                let recent_scores = get_recent_scores(&osu, UserId::Id(member.user_id.unwrap())).await;
                if recent_scores.is_empty() {
                    println!("No recent scores for user {}", member.user_id.unwrap());
                    return;
                }
                if (recent_scores.iter().all(|score| processed_scores.contains(&score.id))) {
                    println!("All scores for user {} are already processed", member.user_id.unwrap());
                    return;
                }
                if !path::Path::new("processed_scores.json").exists() {
                    save_processed_scores(Vec::<u64>::new()).await;
                }
                
                for score in recent_scores {
                    if !processed_scores.contains(&score.id) {
                        processed_scores.push(score.id);
                        send_discord(&osu,score.clone()).await;
                    }
                }
                save_processed_scores(processed_scores).await;
            });
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        println!("Sleeping for 30 seconds...");
    }
}
