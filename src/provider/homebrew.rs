use serde_json::json;

use serde::{Serialize, Deserialize};
use std::convert::TryInto;
use std::env;
use std::thread;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use rouille::Request;
use rouille::Response;
use rouille::post_input;
use rouille::session;
use rouille::try_or_400;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio_postgres::{Error, Row};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use postgres_openssl::MakeTlsConnector;

// Can have multiple homebrew instruments
// Support temperature humidity, windspeed, wind direction, percipitation, PM2.5, PM10, C02, TVOC, etc.
// Must select storage location for homebrew instruments (local, postgres, etc.)
// Multiple instruments can form an inside/outside average
// Instrument can be inside or outside
// Instruments POST to homebrew API using an API key





#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub apikey: String,
    pub pg: PostgresServer,
    pub port: u16
}
impl Config {
    pub async fn init(&self){

        self.build_tables().await;

        let config = self.clone();
        thread::spawn(move || {
            rouille::start_server(format!("0.0.0.0:{}", config.port).as_str(), move |request| {
    
    
                let auth_header = request.header("Authorization");
    
                if auth_header.is_none(){
                    return Response::empty_404();
                } else {
                    if auth_header.unwrap().to_string() != config.apikey{
                        return Response::empty_404();
                    }
                }
    
                if request.url() == "/api/weather_reports" {
                    if request.method() == "POST" {

                        // Collect input params from post request
                        let input = try_or_400!(post_input!(request, {
                            temperature: Option<f64>,
                            humidity: Option<f64>,
                            percipitation: Option<f64>,
                            pm10: Option<f64>,
                            pm25: Option<f64>,
                            co2: Option<f64>,
                            tvoc: Option<f64>,
                            device_type: String,
                        }));

                        let mut obj = WeatherReport::new();
                        obj.temperature = input.temperature;
                        obj.humidity = input.humidity;
                        obj.percipitation = input.percipitation;
                        obj.pm10 = input.pm10;
                        obj.pm25 = input.pm25;
                        obj.co2 = input.co2;
                        obj.tvoc = input.tvoc;
                        obj.device_type = input.device_type.to_string();
                        obj.save(config.clone());
                        return Response::json(&obj);
                    }
                    if request.method() == "GET" {
                        let objects = WeatherReport::select(config.clone(), Some(1), None, Some(format!("timestamp DESC")), None).unwrap();
                        return Response::json(&objects[0].clone());
                    }
                }
    
    
                let mut response = Response::text("hello world");

                return response;
            });
        });
    }

    pub async fn build_tables(&self) -> Result<(), Error>{
    
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        let connector = MakeTlsConnector::new(builder.build());
    
        let (client, connection) = tokio_postgres::connect(format!("postgresql://{}:{}@{}/{}?sslmode=prefer", &self.pg.username, &self.pg.password, &self.pg.address, &self.pg.db_name).as_str(), connector).await?;
        
        // The connection object performs the actual communication with the database,
        // so spawn it off to run on its own.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
    
        // Build WeatherReport Table
        // ---------------------------------------------------------------
        let db = client.batch_execute(WeatherReport::sql_build_statement()).await;
        match db {
            Ok(_v) => log::info!("POSTGRES: CREATED WeatherReport Table"),
            Err(e) => log::error!("POSTGRES: {:?}", e),
        }
        let db_migrations = WeatherReport::migrations();
        for migration in db_migrations {
            let migrations_db = client.batch_execute(migration).await;
            match migrations_db {
                Ok(_v) => log::info!("POSTGRES: Migration Successful"),
                Err(e) => log::error!("POSTGRES: {:?}", e),
            }
        }

        return Ok(());
    }    

}

// Stored in SQL in cache_timeout is set
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeatherReport {
    pub id: i32,
    pub oid: String,
    pub temperature: Option<f64>, // Stored in celcius....api converts to F/C
    pub humidity: Option<f64>,
    pub percipitation: Option<f64>,
    pub pm10: Option<f64>,
    pub pm25: Option<f64>,
    pub co2: Option<f64>,
    pub tvoc: Option<f64>,
    pub device_type: String, // indoor, outdoor, other
    pub timestamp: i64
}
impl WeatherReport {
    pub fn new() -> WeatherReport {
        let oid: String = thread_rng().sample_iter(&Alphanumeric).take(15).map(char::from).collect();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        WeatherReport { 
            id: 0,
            oid: oid,
            temperature: None,
            humidity: None,
            percipitation: None,
            pm10: None,
            pm25: None,
            co2: None,
            tvoc: None,
            device_type: String::from("other"),
            timestamp: timestamp
        }
    }
    pub fn sql_table_name() -> String {
        return format!("weather_reports")
    }
    pub fn sql_build_statement() -> &'static str {
        "CREATE TABLE public.weather_reports (
            id serial NOT NULL,
            oid varchar NOT NULL UNIQUE,
            temperature DOUBLE PRECISION NULL,
            humidity DOUBLE PRECISION NULL,
            percipitation DOUBLE PRECISION NULL,
            pm10 DOUBLE PRECISION NULL,
            pm25 DOUBLE PRECISION NULL,
            co2 DOUBLE PRECISION NULL,
            tvoc DOUBLE PRECISION NULL,
            device_type VARCHAR NULL,
            timestamp BIGINT DEFAULT 0,
            CONSTRAINT weather_reports_pkey PRIMARY KEY (id));"
    }
    pub fn migrations() -> Vec<&'static str> {
        vec![
            "",
        ]
    }
    pub fn save(&self, config: Config) -> Result<&Self, Error>{
        // Get a copy of the master key and postgres info
        let postgres = config.pg.clone();

        // Build SQL adapter that skips verification for self signed certificates
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);

        // Build connector with the adapter from above
        let connector = MakeTlsConnector::new(builder.build());

        // Build postgres client
        let mut client = crate::postgres::Client::connect(format!("postgresql://{}:{}@{}/{}?sslmode=prefer", &postgres.username, &postgres.password, &postgres.address, &postgres.db_name).as_str(), connector)?;

        // Search for OID matches
        let rows = Self::select(
            config.clone(), 
            None, 
            None, 
            None, 
            Some(format!("oid = '{}'", 
                &self.oid, 
            ))
        ).unwrap();

        if rows.len() == 0 {
            client.execute("INSERT INTO weather_reports (oid, device_type, timestamp) VALUES ($1, $2, $3)",
                &[&self.oid.clone(),
                &self.device_type,
                &self.timestamp]
            ).unwrap();
        } 

        if self.temperature.is_some() {
            client.execute("UPDATE weather_reports SET temperature = $1 WHERE oid = $2;", 
            &[
                &self.temperature.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.humidity.is_some() {
            client.execute("UPDATE weather_reports SET humidity = $1 WHERE oid = $2;", 
            &[
                &self.humidity.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.percipitation.is_some() {
            client.execute("UPDATE weather_reports SET percipitation = $1 WHERE oid = $2;", 
            &[
                &self.percipitation.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.pm10.is_some() {
            client.execute("UPDATE weather_reports SET pm10 = $1 WHERE oid = $2;", 
            &[
                &self.pm10.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.pm25.is_some() {
            client.execute("UPDATE weather_reports SET pm25 = $1 WHERE oid = $2;", 
            &[
                &self.pm25.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.co2.is_some() {
            client.execute("UPDATE weather_reports SET co2 = $1 WHERE oid = $2;", 
            &[
                &self.co2.clone().unwrap(),
                &self.oid
            ])?;
        }

        if self.tvoc.is_some() {
            client.execute("UPDATE weather_reports SET tvoc = $1 WHERE oid = $2;", 
            &[
                &self.tvoc.clone().unwrap(),
                &self.oid
            ])?;
        }

        return Ok(self);
    }
    pub fn select(config: Config, limit: Option<usize>, offset: Option<usize>, order: Option<String>, query: Option<String>) -> Result<Vec<Self>, Error>{
        
    
        // Get a copy of the master key and postgres info
        let postgres = config.pg.clone();
            
        let mut execquery = "SELECT * FROM weather_reports".to_string();

        match query {
            Some(query_val) => {
                execquery = format!("{} {} {}", execquery.clone(), "WHERE", query_val);
            },
            None => {
                
            }
        }
        match order {
            Some(order_val) => {
                execquery = format!("{} {} {}", execquery.clone(), "ORDER BY", order_val);
            },
            None => {
                execquery = format!("{} {} {}", execquery.clone(), "ORDER BY", "id DESC");
            }
        }
        match limit {
            Some(limit_val) => {
                execquery = format!("{} {} {}", execquery.clone(), "LIMIT", limit_val);
            },
            None => {}
        }
        match offset {
            Some(offset_val) => {
                execquery = format!("{} {} {}", execquery.clone(), "OFFSET", offset_val);
            },
            None => {}
        }

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        builder.set_verify(SslVerifyMode::NONE);
        let connector = MakeTlsConnector::new(builder.build());
        let mut client = crate::postgres::Client::connect(format!("postgresql://{}:{}@{}/{}?sslmode=prefer", &postgres.username, &postgres.password, &postgres.address, &postgres.db_name).as_str(), connector)?;

        let mut parsed_rows: Vec<Self> = Vec::new();
        for row in client.query(execquery.as_str(), &[])? {
            parsed_rows.push(Self::from_row(&row).unwrap());
        }

        return Ok(parsed_rows);
    }
    fn from_row(row: &Row) -> Result<Self, Error> {
        return Ok(Self {
            id: row.get("id"),
            oid: row.get("oid"),
            temperature: row.get("temperature"),
            humidity: row.get("humidity"),
            percipitation: row.get("percipitation"),
            pm10: row.get("pm10"),
            pm25: row.get("pm25"),
            co2: row.get("co2"),
            tvoc: row.get("tvoc"),
            device_type: row.get("device_type"),
            timestamp: row.get("timestamp"),
        });
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PostgresServer {
	pub db_name: String,
    pub username: String,
    pub password: String,
	pub address: String
}
impl PostgresServer {
    pub fn new() -> PostgresServer {

        let db_name = env::var("HOMEBREW_PG_DBNAME").expect("$HOMEBREW_PG_DBNAME is not set");
        let username = env::var("HOMEBREW_PG_USER").expect("$HOMEBREW_PG_USER is not set");
        let password = env::var("HOMEBREW_PG_PASS").expect("$HOMEBREW_PG_PASS is not set");
        let address = env::var("HOMEBREW_PG_ADDRESS").expect("$HOMEBREW_PG_ADDRESS is not set");


        PostgresServer{
            db_name, 
            username, 
            password, 
            address
        }
    }
}