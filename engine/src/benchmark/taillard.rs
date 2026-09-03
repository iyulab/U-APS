//! Taillard benchmark instance parser
//!
//! Parses standard Taillard JSP instances (ta01-ta80)

use crate::models::{Job, Operation, Resource};

/// Taillard instance data
#[derive(Debug, Clone)]
pub struct TaillardInstance {
    /// Instance name (e.g., "ta01")
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

impl TaillardInstance {
    /// Parse Taillard format string
    ///
    /// Format:
    /// ```text
    /// num_jobs num_machines
    /// machine_0 time_0 machine_1 time_1 ... (for job 0)
    /// machine_0 time_0 machine_1 time_1 ... (for job 1)
    /// ...
    /// ```
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

        // Create resources (machines)
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
                return Err(format!(
                    "Job {} has insufficient data: expected {} values, got {}",
                    job_idx,
                    num_machines * 2,
                    values.len()
                ));
            }

            let job_id = format!("J{}", job_idx);
            let mut job = Job::new(&job_id);

            // Parse operations (machine, time pairs)
            for op_idx in 0..num_machines {
                let machine = values[op_idx * 2] as usize;
                let process_time = values[op_idx * 2 + 1];

                let op_id = format!("{}-O{}", job_id, op_idx);
                let operation = Operation::new(&op_id, &job_id, op_idx as i32 + 1)
                    .with_time(0, process_time * 1000, 0) // Convert to ms
                    .with_equipment(vec![format!("M{}", machine)]);

                job = job.with_operation(operation);
            }

            jobs.push(job);
        }

        Ok(TaillardInstance {
            name: name.to_string(),
            num_jobs,
            num_machines,
            jobs,
            resources,
            best_known: None,
        })
    }

    /// Set the best known solution for comparison
    pub fn with_best_known(mut self, makespan_seconds: i64) -> Self {
        self.best_known = Some(makespan_seconds * 1000); // Convert to ms
        self
    }
}

/// Best known solutions for Taillard instances
pub fn get_taillard_bks(instance_name: &str) -> Option<i64> {
    match instance_name {
        // 15x15 instances
        "ta01" => Some(1231),
        "ta02" => Some(1244),
        "ta03" => Some(1218),
        "ta04" => Some(1175),
        "ta05" => Some(1224),
        "ta06" => Some(1238),
        "ta07" => Some(1227),
        "ta08" => Some(1217),
        "ta09" => Some(1274),
        "ta10" => Some(1241),
        // 20x15 instances
        "ta11" => Some(1357),
        "ta12" => Some(1367),
        "ta13" => Some(1342),
        "ta14" => Some(1345),
        "ta15" => Some(1339),
        "ta16" => Some(1360),
        "ta17" => Some(1462),
        "ta18" => Some(1396),
        "ta19" => Some(1332),
        "ta20" => Some(1348),
        // 20x20 instances
        "ta21" => Some(1642),
        "ta22" => Some(1600),
        "ta23" => Some(1557),
        "ta24" => Some(1644),
        "ta25" => Some(1595),
        "ta26" => Some(1643),
        "ta27" => Some(1680),
        "ta28" => Some(1603),
        "ta29" => Some(1625),
        "ta30" => Some(1584),
        // 30x15 instances
        "ta31" => Some(1764),
        "ta32" => Some(1784),
        "ta33" => Some(1791),
        "ta34" => Some(1829),
        "ta35" => Some(2007),
        "ta36" => Some(1819),
        "ta37" => Some(1771),
        "ta38" => Some(1673),
        "ta39" => Some(1795),
        "ta40" => Some(1631),
        // 30x20 instances
        "ta41" => Some(2005),
        "ta42" => Some(1937),
        "ta43" => Some(1846),
        "ta44" => Some(1979),
        "ta45" => Some(2000),
        "ta46" => Some(1940),
        "ta47" => Some(1789),
        "ta48" => Some(1912),
        "ta49" => Some(1931),
        "ta50" => Some(1923),
        // 50x15 instances
        "ta51" => Some(2760),
        "ta52" => Some(2756),
        "ta53" => Some(2717),
        "ta54" => Some(2839),
        "ta55" => Some(2679),
        "ta56" => Some(2781),
        "ta57" => Some(2943),
        "ta58" => Some(2885),
        "ta59" => Some(2655),
        "ta60" => Some(2723),
        // 50x20 instances
        "ta61" => Some(2868),
        "ta62" => Some(2869),
        "ta63" => Some(2755),
        "ta64" => Some(2702),
        "ta65" => Some(2725),
        "ta66" => Some(2845),
        "ta67" => Some(2825),
        "ta68" => Some(2784),
        "ta69" => Some(3071),
        "ta70" => Some(2995),
        // 100x20 instances
        "ta71" => Some(5464),
        "ta72" => Some(5181),
        "ta73" => Some(5568),
        "ta74" => Some(5339),
        "ta75" => Some(5392),
        "ta76" => Some(5342),
        "ta77" => Some(5436),
        "ta78" => Some(5394),
        "ta79" => Some(5358),
        "ta80" => Some(5183),
        _ => None,
    }
}

/// Generate a simple Taillard-style instance for testing
pub fn generate_test_instance(num_jobs: usize, num_machines: usize) -> TaillardInstance {
    use rand::prelude::*;
    let mut rng = rand::rng();

    let resources: Vec<Resource> = (0..num_machines)
        .map(|i| Resource::equipment(format!("M{}", i)))
        .collect();

    let mut jobs = Vec::with_capacity(num_jobs);

    for job_idx in 0..num_jobs {
        let job_id = format!("J{}", job_idx);
        let mut job = Job::new(&job_id);

        // Random permutation of machines
        let mut machines: Vec<usize> = (0..num_machines).collect();
        machines.shuffle(&mut rng);

        for (op_idx, &machine) in machines.iter().enumerate() {
            let process_time = rng.random_range(1..100) * 1000; // 1-99 seconds in ms

            let op_id = format!("{}-O{}", job_id, op_idx);
            let operation = Operation::new(&op_id, &job_id, op_idx as i32 + 1)
                .with_time(0, process_time, 0)
                .with_equipment(vec![format!("M{}", machine)]);

            job = job.with_operation(operation);
        }

        jobs.push(job);
    }

    TaillardInstance {
        name: format!("test_{}x{}", num_jobs, num_machines),
        num_jobs,
        num_machines,
        jobs,
        resources,
        best_known: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_instance() {
        let content = r#"
            3 3
            0 10 1 20 2 30
            1 15 0 25 2 35
            2 5 1 10 0 15
        "#;

        let instance = TaillardInstance::parse("test", content).unwrap();

        assert_eq!(instance.num_jobs, 3);
        assert_eq!(instance.num_machines, 3);
        assert_eq!(instance.jobs.len(), 3);
        assert_eq!(instance.resources.len(), 3);

        // Check first job's first operation
        let first_op = &instance.jobs[0].operations[0];
        assert_eq!(first_op.time.process_ms, 10_000); // 10 seconds in ms
    }

    #[test]
    fn test_bks_lookup() {
        assert_eq!(get_taillard_bks("ta01"), Some(1231));
        assert_eq!(get_taillard_bks("ta80"), Some(5183));
        assert_eq!(get_taillard_bks("invalid"), None);
    }

    #[test]
    fn test_generate_instance() {
        let instance = generate_test_instance(5, 3);

        assert_eq!(instance.num_jobs, 5);
        assert_eq!(instance.num_machines, 3);
        assert_eq!(instance.jobs.len(), 5);

        // Each job should have 3 operations
        for job in &instance.jobs {
            assert_eq!(job.operations.len(), 3);
        }
    }

    #[test]
    fn test_with_best_known() {
        let instance = generate_test_instance(3, 3).with_best_known(100);

        assert_eq!(instance.best_known, Some(100_000)); // 100 seconds in ms
    }
}
