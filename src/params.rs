use anyhow::Result;

pub trait AssetParams: Send + Sync {
    fn primary_source(&self) -> &str;

    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

#[macro_export]
macro_rules! impl_as_any {
    ($t:ty) => {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

pub fn downcast_params<'a, T: 'static>(
    params: &'a dyn AssetParams,
    category: &'a str,
) -> Result<&'a T> {
    params.as_any().downcast_ref::<T>().ok_or_else(|| {
        anyhow::anyhow!(
            "internal error: params type mismatch for category '{category}', this is a bug in the category registration"
        )
    })
}

pub fn downcast_params_mut<'a, T: 'static>(
    params: &'a mut dyn AssetParams,
    category: &'a str,
) -> Result<&'a mut T> {
    params.as_any_mut().downcast_mut::<T>().ok_or_else(|| {
        anyhow::anyhow!("internal error: params mutable downcast mismatch for '{category}'")
    })
}
