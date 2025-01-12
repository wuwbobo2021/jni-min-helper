package rust.jniminhelper;

import android.content.Context;
import android.content.Intent;
import android.content.BroadcastReceiver;

public class BroadcastRec extends BroadcastReceiver {
    public interface BroadcastRecHdl {
        public void onReceive(Context context, Intent intent);
    }

    BroadcastRecHdl hdl;
    public BroadcastRec(BroadcastRecHdl hdl) {
        this.hdl = hdl;
    }
 
    @Override
    public void onReceive(Context context, Intent intent) {
        if (this.hdl != null) {
            this.hdl.onReceive(context, intent);
        }
    }
}
