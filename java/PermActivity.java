package rust.jniminhelper;

import android.app.Activity;
import android.os.Bundle;
import android.content.Intent;

public class PermActivity extends Activity {
    static final String EXTRA_PERM_ARRAY = "rust.jniminhelper.perm_array";
    static final String EXTRA_TITLE = "rust.jniminhelper.perm_activity_title";
    
    // to be registered in native code
    private native void nativeOnRequestPermissionsResult(String[] permissions, int[] grantResults);

	@Override
	protected void onCreate(Bundle savedInstanceState) {
	    super.onCreate(savedInstanceState);
	    Intent intent = this.getIntent();
	    String[] permissions = intent.getStringArrayExtra(EXTRA_PERM_ARRAY);
	    this.requestPermissions(permissions, 0);
	}
	
	@Override
	protected void onStart() {
	    super.onStart();
	    Intent intent = this.getIntent();
	    String title = intent.getStringExtra(EXTRA_TITLE);
	    this.setTitle(title);
	}
	
    @Override
    public void onRequestPermissionsResult(int requestCode,
        String[] permissions, int[] grantResults)
    {
        this.nativeOnRequestPermissionsResult(permissions, grantResults);
        this.finish();
    }
}
