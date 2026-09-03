//! Lawrence benchmark instance parser
//!
//! Parses standard Lawrence JSP instances (la01-la40)

use crate::models::{Job, Operation, Resource};

/// Lawrence instance data
#[derive(Debug, Clone)]
pub struct LawrenceInstance {
    /// Instance name (e.g., "la01")
    pub name: String,
    /// Number of jobs
    pub num_jobs: usize,
    /// Number of machines
    pub num_machines: usize,
    /// Jobs with operations
    pub jobs: Vec<Job>,
    /// Resources (machines)
    pub resources: Vec<Resource>,
    /// Best known solution (makespan)
    pub best_known: Option<i64>,
}

impl LawrenceInstance {
    /// Parse Lawrence format string (same as Taillard format)
    pub fn parse(name: &str, content: &str) -> Result<Self, String> {
        let lines: Vec<&str> = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();

        if lines.is_empty() {
            return Err("Empty input".to_string());
        }

        // Parse header
        let header: Vec<usize> = lines[0]
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();

        if header.len() < 2 {
            return Err("Invalid header format".to_string());
        }

        let num_jobs = header[0];
        let num_machines = header[1];

        // Create resources
        let resources: Vec<Resource> = (0..num_machines)
            .map(|i| Resource::equipment(format!("M{}", i)))
            .collect();

        // Parse jobs
        let mut jobs = Vec::with_capacity(num_jobs);

        for (job_idx, line) in lines[1..].iter().enumerate().take(num_jobs) {
            let values: Vec<i64> = line
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() < num_machines * 2 {
                return Err(format!("Job {} has insufficient data", job_idx));
            }

            let job_id = format!("J{}", job_idx);
            let mut job = Job::new(&job_id);

            for op_idx in 0..num_machines {
                let machine = values[op_idx * 2] as usize;
                let process_time = values[op_idx * 2 + 1];

                let op_id = format!("{}-O{}", job_id, op_idx);
                let operation = Operation::new(&op_id, &job_id, op_idx as i32 + 1)
                    .with_time(0, process_time * 1000, 0)
                    .with_equipment(vec![format!("M{}", machine)]);

                job = job.with_operation(operation);
            }

            jobs.push(job);
        }

        Ok(LawrenceInstance {
            name: name.to_string(),
            num_jobs,
            num_machines,
            jobs,
            resources,
            best_known: None,
        })
    }

    /// Set the best known solution
    pub fn with_best_known(mut self, makespan_seconds: i64) -> Self {
        self.best_known = Some(makespan_seconds * 1000);
        self
    }
}

/// Best known solutions for Lawrence instances
pub fn get_lawrence_bks(instance_name: &str) -> Option<i64> {
    match instance_name {
        // 10x5 instances
        "la01" => Some(666),
        "la02" => Some(655),
        "la03" => Some(597),
        "la04" => Some(590),
        "la05" => Some(593),
        // 15x5 instances
        "la06" => Some(926),
        "la07" => Some(890),
        "la08" => Some(863),
        "la09" => Some(951),
        "la10" => Some(958),
        // 20x5 instances
        "la11" => Some(1222),
        "la12" => Some(1039),
        "la13" => Some(1150),
        "la14" => Some(1292),
        "la15" => Some(1207),
        // 10x10 instances
        "la16" => Some(945),
        "la17" => Some(784),
        "la18" => Some(848),
        "la19" => Some(842),
        "la20" => Some(902),
        // 15x10 instances
        "la21" => Some(1046),
        "la22" => Some(927),
        "la23" => Some(1032),
        "la24" => Some(935),
        "la25" => Some(977),
        // 20x10 instances
        "la26" => Some(1218),
        "la27" => Some(1235),
        "la28" => Some(1216),
        "la29" => Some(1152),
        "la30" => Some(1355),
        // 30x10 instances
        "la31" => Some(1784),
        "la32" => Some(1850),
        "la33" => Some(1719),
        "la34" => Some(1721),
        "la35" => Some(1888),
        // 15x15 instances
        "la36" => Some(1268),
        "la37" => Some(1397),
        "la38" => Some(1196),
        "la39" => Some(1233),
        "la40" => Some(1222),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lawrence() {
        let content = r#"
            2 3
            0 5 1 10 2 15
            2 8 0 12 1 6
        "#;

        let instance = LawrenceInstance::parse("test", content).unwrap();

        assert_eq!(instance.num_jobs, 2);
        assert_eq!(instance.num_machines, 3);
    }

    #[test]
    fn test_lawrence_bks() {
        assert_eq!(get_lawrence_bks("la01"), Some(666));
        assert_eq!(get_lawrence_bks("la40"), Some(1222));
        assert_eq!(get_lawrence_bks("invalid"), None);
    }
}
