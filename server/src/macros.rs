macro_rules! clone_params {
  ($($param:tt),*) => {
    ($($param.clone()),*)
  };
}

macro_rules! schedule_job {
  ($scheduler:ident, $job_name:expr, $job_schedule:expr, $job_fn:expr, $($params:tt),*) => {
      use tokio_cron_scheduler::Job;
      #[derive(Debug, Clone)]
      struct JobConfig {
          name: String,
          schedule: String,
      }

      let job_config = JobConfig {
        name: $job_name.to_string(),
        schedule: $job_schedule.to_string(),
      };
      tracing::info!("Scheduling job with config {:?}", job_config);
      let run_frequency_cron = job_config.schedule.clone();
      let $($params),* = clone_params!($($params),*);
      $scheduler
      .add(Job::new_async(
        run_frequency_cron.as_str(),
        move |uuid, mut l: JobScheduler| {
          let $($params),* = clone_params!($($params),*);
          let job_config = job_config.clone();
          Box::pin(async move {
            let next_tick = l.next_tick_for_job(uuid).await;
            match next_tick {
              Ok(Some(ts)) => tracing::info!("Next time for job is {:?}", ts),
              _ => tracing::info!("Could not get next tick for job"),
            }

            let result = $job_fn($($params),*).await;
            match result {
              Ok(_) => {
                tracing::info!(
                  "Job {} with config {:?} succeeded",
                  uuid,
                  job_config
                )
              }
              Err(e) => {
                tracing::error!("Job fn {} failed: {:?}", stringify!($job_fn), e);
              }
            }
          })
        },
      )?)
      .await?;
  };
}
